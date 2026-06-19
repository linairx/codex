//! Request-routing helpers for northbound v2 client handling.
//!
//! This module owns the request-dispatch branches under `handle_client_request`
//! so `v2.rs` can stay focused on the connection loop and high-level control
//! flow.

use crate::event::GatewayAccountPathHandoffFailed;
use crate::event::GatewayAccountPathHandoffSucceeded;
use crate::event::GatewayAccountThreadHandoffFailed;
use crate::event::GatewayAccountThreadHandoffSucceeded;
use crate::event::GatewayEvent;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_connection::DownstreamWorkerHandle;
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_routing::config_read_response_matches_cwd;
use crate::northbound::v2_routing::path_handoff_response_must_match_request;
use crate::northbound::v2_routing::worker_for_request;
use crate::northbound::v2_routing::worker_for_server_request;
use crate::northbound::v2_scope::apply_response_scope_policy;
use crate::northbound::v2_scope::log_failed_visible_thread_worker_route_recovery;
use crate::northbound::v2_scope::log_recovered_visible_thread_worker_route;
use crate::northbound::v2_scope::request_thread_id;
use crate::northbound::v2_scope::request_thread_path;
use crate::northbound::v2_scope::response_thread_id;
use crate::northbound::v2_scope::response_thread_path;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::request_params;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::FsUnwatchParams;
use codex_app_server_protocol::FsWatchParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadReadParams;
use serde_json::Value;
use std::io;
use std::path::Path;
use tracing::info;
use tracing::warn;

pub(crate) async fn recover_visible_thread_worker_route(
    downstream: &GatewayV2DownstreamRouter,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    thread_id: &str,
) -> io::Result<()> {
    let attempted_worker_ids = downstream
        .workers
        .iter()
        .map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let attempted_worker_websocket_urls = downstream
        .workers
        .iter()
        .map(|worker| downstream.websocket_url_for_worker_id(worker.worker_id))
        .collect::<Vec<_>>();
    for worker in &downstream.workers {
        let response = worker
            .request_handle
            .request(ClientRequest::ThreadRead {
                request_id: RequestId::String(format!("gateway-thread-route-probe:{thread_id}")),
                params: ThreadReadParams {
                    thread_id: thread_id.to_string(),
                    include_turns: false,
                },
            })
            .await?;
        if let Ok(result) = response
            && response_thread_id(&result) == Some(thread_id)
        {
            scope_registry.register_thread_with_worker(
                thread_id.to_string(),
                context.clone(),
                worker.worker_id,
            );
            log_recovered_visible_thread_worker_route(
                context,
                thread_id,
                worker.worker_id,
                downstream.websocket_url_for_worker_id(worker.worker_id),
            );
            observability.record_v2_thread_route_recovery("success");
            break;
        }
    }

    if scope_registry.thread_worker_id(thread_id).is_none() {
        log_failed_visible_thread_worker_route_recovery(
            context,
            thread_id,
            &attempted_worker_ids,
            &attempted_worker_websocket_urls,
        );
        observability.record_v2_thread_route_recovery("miss");
    }

    Ok(())
}

pub(crate) async fn route_thread_start_request_with_account_capacity_failover(
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let max_attempts = downstream.workers.len().max(1);
    let mut first_capacity_error = None;
    let mut exhausted_worker_ids = Vec::new();

    for _ in 0..max_attempts {
        let worker = worker_for_request(
            downstream,
            connection.scope_registry,
            connection.request_context,
            &request,
        )?
        .clone();
        let worker_account_id = worker
            .worker_id
            .and_then(|worker_id| downstream.worker_account_id(worker_id));
        let method = request.method.clone();
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;

        match response {
            Ok(result) => {
                if let Some(worker_id) = worker.worker_id {
                    downstream.mark_worker_account_available(worker_id);
                    if !exhausted_worker_ids.is_empty() {
                        connection.observability.publish_operator_event(
                            GatewayEvent::account_failover_succeeded(
                                connection.request_context.tenant_id.as_str(),
                                connection.request_context.project_id.as_deref(),
                                worker_id,
                                downstream.worker_account_id(worker_id),
                                exhausted_worker_ids.clone(),
                            ),
                        );
                        connection.observability.record_v2_account_capacity_event(
                            worker_id,
                            "thread_start_failover_success",
                            Some(connection.request_context),
                            Some("thread/start restored on a replacement account-backed worker"),
                        );
                        info!(
                            method = method.as_str(),
                            tenant_id = connection.request_context.tenant_id.as_str(),
                            project_id = connection.request_context.project_id.as_deref(),
                            replacement_worker_id = worker_id,
                            replacement_account_id = downstream.worker_account_id(worker_id),
                            exhausted_worker_ids = ?exhausted_worker_ids,
                            "gateway v2 thread/start retried on another account-backed worker after account capacity exhaustion"
                        );
                    }
                }
                return Ok(Ok(apply_response_scope_policy(
                    connection.scope_registry,
                    connection.request_context,
                    Some(connection.observability),
                    &method,
                    worker.worker_id,
                    worker_account_id,
                    result,
                )?));
            }
            Err(error) if is_account_capacity_error(&error) => {
                if let Some(worker_id) = worker.worker_id {
                    connection.observability.record_v2_account_capacity_event(
                        worker_id,
                        "exhausted",
                        Some(connection.request_context),
                        Some(error.message.as_str()),
                    );
                    warn!(
                        method = method.as_str(),
                        tenant_id = connection.request_context.tenant_id.as_str(),
                        project_id = connection.request_context.project_id.as_deref(),
                        exhausted_worker_id = worker_id,
                        exhausted_account_id = downstream.worker_account_id(worker_id),
                        reason = error.message.as_str(),
                        "gateway v2 marked account-backed worker exhausted after downstream thread/start failure"
                    );
                    connection.observability.publish_operator_event(
                        GatewayEvent::account_capacity_exhausted(
                            connection.request_context.tenant_id.as_str(),
                            connection.request_context.project_id.as_deref(),
                            worker_id,
                            downstream.worker_account_id(worker_id),
                            error.message.as_str(),
                        ),
                    );
                    downstream.mark_worker_account_exhausted(worker_id, error.message.clone());
                    exhausted_worker_ids.push(worker_id);
                }
                if first_capacity_error.is_none() {
                    first_capacity_error = Some(error);
                }
            }
            Err(error) => return Ok(Err(error)),
        }
    }

    first_capacity_error.map(Err).ok_or_else(|| {
        io::Error::other(
            "gateway v2 connection has no downstream app-server sessions with available account capacity",
        )
    })
}

pub(crate) async fn route_thread_id_request_with_account_handoff(
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let method = request.method.clone();
    let Some((success_metric, failure_metric)) = thread_id_account_handoff_metrics(method.as_str())
    else {
        return Err(io::Error::other(format!(
            "gateway v2 {method} handoff is not supported"
        )));
    };
    let Some(thread_id) = request_thread_id(&request).map(str::to_string) else {
        return Err(io::Error::other(format!(
            "gateway v2 {method} handoff requires a thread id"
        )));
    };
    let Some(exhausted_worker_id) = connection.scope_registry.thread_worker_id(&thread_id) else {
        let worker = worker_for_request(
            downstream,
            connection.scope_registry,
            connection.request_context,
            &request,
        )?
        .clone();
        let worker_account_id = worker
            .worker_id
            .and_then(|worker_id| downstream.worker_account_id(worker_id));
        return send_thread_request_to_worker(
            &worker,
            connection.scope_registry,
            connection.request_context,
            Some(connection.observability),
            worker_account_id,
            request,
        )
        .await;
    };

    if downstream.worker_account_has_capacity(exhausted_worker_id) {
        let worker = worker_for_request(
            downstream,
            connection.scope_registry,
            connection.request_context,
            &request,
        )?
        .clone();
        let worker_account_id = worker
            .worker_id
            .and_then(|worker_id| downstream.worker_account_id(worker_id));
        return send_thread_request_to_worker(
            &worker,
            connection.scope_registry,
            connection.request_context,
            Some(connection.observability),
            worker_account_id,
            request,
        )
        .await;
    }

    let mut first_error = None;
    for worker in &downstream.workers {
        let Some(replacement_worker_id) = worker.worker_id else {
            continue;
        };
        if replacement_worker_id == exhausted_worker_id
            || !downstream.worker_is_healthy(replacement_worker_id)
            || !downstream.worker_account_has_capacity(replacement_worker_id)
        {
            continue;
        }

        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if matches!(
                    method.as_str(),
                    "getConversationSummary"
                        | "thread/metadata/update"
                        | "thread/read"
                        | "thread/resume"
                        | "thread/rollback"
                        | "thread/unarchive"
                ) && response_thread_id(&result) != Some(thread_id.as_str())
                {
                    if first_error.is_none() {
                        first_error = Some(JSONRPCErrorError {
                            code: INVALID_PARAMS_CODE,
                            message: format!(
                                "replacement worker returned a different thread while restoring {thread_id}"
                            ),
                            data: None,
                        });
                    }
                    continue;
                }
                connection.observability.record_v2_account_capacity_event(
                    replacement_worker_id,
                    success_metric,
                    Some(connection.request_context),
                    Some("thread id request restored on a replacement account-backed worker"),
                );
                connection.observability.publish_operator_event(
                    GatewayEvent::account_thread_handoff_succeeded(
                        GatewayAccountThreadHandoffSucceeded {
                            tenant_id: connection.request_context.tenant_id.as_str(),
                            project_id: connection.request_context.project_id.as_deref(),
                            method: method.as_str(),
                            thread_id: thread_id.as_str(),
                            exhausted_worker_id,
                            exhausted_account_id: downstream.worker_account_id(exhausted_worker_id),
                            replacement_worker_id,
                            replacement_account_id: downstream
                                .worker_account_id(replacement_worker_id),
                        },
                    ),
                );
                if matches!(
                    method.as_str(),
                    "thread/archive"
                        | "thread/compact/start"
                        | "thread/decrement_elicitation"
                        | "thread/increment_elicitation"
                        | "thread/inject_items"
                        | "thread/memoryMode/set"
                        | "thread/name/set"
                        | "thread/rollback"
                        | "thread/turns/list"
                        | "thread/unsubscribe"
                ) {
                    connection.scope_registry.register_thread_with_worker(
                        thread_id.clone(),
                        connection.request_context.clone(),
                        worker.worker_id,
                    );
                }
                info!(
                    method = method.as_str(),
                    tenant_id = connection.request_context.tenant_id.as_str(),
                    project_id = connection.request_context.project_id.as_deref(),
                    thread_id,
                    exhausted_worker_id,
                    exhausted_account_id = downstream.worker_account_id(exhausted_worker_id),
                    replacement_worker_id,
                    replacement_account_id = downstream.worker_account_id(replacement_worker_id),
                    "gateway v2 restored thread id request on another account-backed worker after account capacity exhaustion"
                );
                return Ok(Ok(apply_response_scope_policy(
                    connection.scope_registry,
                    connection.request_context,
                    Some(connection.observability),
                    method.as_str(),
                    worker.worker_id,
                    worker
                        .worker_id
                        .and_then(|worker_id| downstream.worker_account_id(worker_id)),
                    result,
                )?));
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    let message = format!(
        "thread {thread_id} is pinned to worker {exhausted_worker_id} with exhausted account capacity for {method}, and no replacement worker restored the context"
    );
    connection.observability.record_v2_account_capacity_event(
        exhausted_worker_id,
        failure_metric,
        Some(connection.request_context),
        Some(message.as_str()),
    );
    connection
        .observability
        .publish_operator_event(GatewayEvent::account_thread_handoff_failed(
            GatewayAccountThreadHandoffFailed {
                tenant_id: connection.request_context.tenant_id.as_str(),
                project_id: connection.request_context.project_id.as_deref(),
                method: method.as_str(),
                thread_id: thread_id.as_str(),
                exhausted_worker_id,
                exhausted_account_id: downstream.worker_account_id(exhausted_worker_id),
                reason: message.as_str(),
            },
        ));
    warn!(
        method = method.as_str(),
        account_capacity_event = failure_metric,
        tenant_id = connection.request_context.tenant_id.as_str(),
        project_id = connection.request_context.project_id.as_deref(),
        thread_id,
        exhausted_worker_id,
        exhausted_account_id = downstream.worker_account_id(exhausted_worker_id),
        first_error = ?first_error,
        "gateway v2 failed to restore thread id request on another account-backed worker after account capacity exhaustion"
    );
    Err(io::Error::other(FailClosedMultiWorkerRouteError {
        message,
    }))
}

pub(crate) async fn send_thread_request_to_worker(
    worker: &DownstreamWorkerHandle,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: Option<&GatewayObservability>,
    worker_account_id: Option<String>,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let method = request.method.clone();
    let response = worker
        .request_handle
        .request(jsonrpc_request_to_client_request(request)?)
        .await?;
    Ok(match response {
        Ok(result) => Ok(apply_response_scope_policy(
            scope_registry,
            context,
            observability,
            &method,
            worker.worker_id,
            worker_account_id,
            result,
        )?),
        Err(error) => Err(error),
    })
}

pub(crate) async fn fanout_mutating_connection_request(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let mut primary_result = None;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if index == 0 {
                    primary_result = Some(result);
                }
            }
            Err(error) => return Ok(Err(error)),
        }
    }

    primary_result.map(Ok).ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })
}

pub(crate) async fn fanout_fs_watch_request(
    downstream: &mut GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let params = request_params::<FsWatchParams>(&request)?;
    let mut primary_result = None;
    let mut watched_workers = Vec::new();

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                watched_workers.push(worker.worker_id);
                if index == 0 {
                    primary_result = Some(result);
                }
            }
            Err(error) => {
                rollback_fs_watch_registrations(downstream, &params.watch_id, &watched_workers)
                    .await;
                return Ok(Err(error));
            }
        }
    }

    let Some(primary_result) = primary_result else {
        return Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        ));
    };

    downstream.record_fs_watch(params);
    Ok(Ok(primary_result))
}

pub(crate) async fn fanout_fs_unwatch_request(
    downstream: &mut GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let params = request_params::<FsUnwatchParams>(&request)?;
    let result = fanout_mutating_connection_request(downstream, request).await?;
    if result.is_ok() {
        downstream.clear_fs_watch(&params.watch_id);
    }
    Ok(result)
}

async fn rollback_fs_watch_registrations(
    downstream: &GatewayV2DownstreamRouter,
    watch_id: &str,
    worker_ids: &[Option<usize>],
) {
    for worker_id in worker_ids {
        let Some(worker) = downstream.latest_worker_with_id(*worker_id) else {
            continue;
        };
        let _ = worker
            .request_handle
            .request(ClientRequest::FsUnwatch {
                request_id: RequestId::String(format!("gateway-rollback-fs-watch:{watch_id}")),
                params: FsUnwatchParams {
                    watch_id: watch_id.to_string(),
                },
            })
            .await;
    }
}

pub(crate) async fn route_config_read_request_if_supported(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let params = request_params::<ConfigReadParams>(&request)?;
    let Some(cwd) = params.cwd.as_ref() else {
        if downstream.multi_worker_topology() {
            downstream.ensure_primary_worker_present_for("config/read")?;
        }
        let worker = downstream.primary_worker()?;
        return worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request)?)
            .await;
    };
    downstream.ensure_all_configured_workers_present_for("config/read")?;
    let request_cwd = Path::new(cwd);
    let mut primary_result = None;
    let mut first_result = None;
    let mut first_error = None;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if config_read_response_matches_cwd(&result, request_cwd) {
                    return Ok(Ok(result));
                }
                if index == 0 {
                    primary_result = Some(result.clone());
                }
                if first_result.is_none() {
                    first_result = Some(result);
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(result) = primary_result.or(first_result) {
        return Ok(Ok(result));
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) async fn first_successful_connection_request(
    downstream: &GatewayV2DownstreamRouter,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let mut first_error = None;

    for worker in &downstream.workers {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => return Ok(Ok(result)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) async fn fanout_connection_notification(
    downstream: &GatewayV2DownstreamRouter,
    notification: ClientNotification,
) -> io::Result<()> {
    for worker in &downstream.workers {
        worker.request_handle.notify(notification.clone()).await?;
    }
    Ok(())
}

pub(crate) async fn first_successful_visible_thread_read_request(
    downstream: &GatewayV2DownstreamRouter,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut first_error = None;

    for worker in &downstream.workers {
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                return Ok(Ok(apply_response_scope_policy(
                    scope_registry,
                    context,
                    None,
                    &request.method,
                    worker.worker_id,
                    worker
                        .worker_id
                        .and_then(|worker_id| downstream.worker_account_id(worker_id)),
                    result,
                )?));
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) async fn first_successful_scoped_path_thread_request(
    downstream: &GatewayV2DownstreamRouter,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;
    let mut exhausted_route_worker_id = None;
    if let Some(thread_path) = request_thread_path(&request)
        && let Some(worker_id) = scope_registry.thread_path_worker_id(thread_path)
    {
        if !downstream.worker_account_has_capacity(worker_id) {
            exhausted_route_worker_id = Some(worker_id);
        } else {
            let response = worker_for_server_request(downstream, Some(worker_id))?
                .request_handle
                .request(jsonrpc_request_to_client_request(request.clone())?)
                .await?;
            return match response {
                Ok(result) => Ok(Ok(apply_response_scope_policy(
                    scope_registry,
                    context,
                    Some(observability),
                    &request.method,
                    Some(worker_id),
                    downstream.worker_account_id(worker_id),
                    result,
                )?)),
                Err(error) => Ok(Err(error)),
            };
        }
    }

    let mut first_error = None;

    for worker in &downstream.workers {
        if worker.worker_id.is_some_and(|worker_id| {
            !downstream.worker_is_healthy(worker_id)
                || !downstream.worker_account_has_capacity(worker_id)
        }) {
            continue;
        }
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;
        match response {
            Ok(result) => {
                if let Some(thread_path) = request_thread_path(&request)
                    && path_handoff_response_must_match_request(&request)
                    && response_thread_path(&result) != Some(thread_path)
                {
                    if first_error.is_none() {
                        first_error = Some(JSONRPCErrorError {
                            code: INVALID_PARAMS_CODE,
                            message: format!(
                                "replacement worker returned a different rollout path while restoring {thread_path}"
                            ),
                            data: None,
                        });
                    }
                    continue;
                }
                if let (Some(exhausted_worker_id), Some(replacement_worker_id)) =
                    (exhausted_route_worker_id, worker.worker_id)
                {
                    observability.record_v2_account_capacity_event(
                        replacement_worker_id,
                        "path_thread_handoff_success",
                        Some(context),
                        Some("path-based thread request restored on a replacement account-backed worker"),
                    );
                    if let Some(thread_path) = request_thread_path(&request) {
                        observability.publish_operator_event(
                            GatewayEvent::account_path_handoff_succeeded(
                                GatewayAccountPathHandoffSucceeded {
                                    tenant_id: context.tenant_id.as_str(),
                                    project_id: context.project_id.as_deref(),
                                    method: request.method.as_str(),
                                    thread_path,
                                    exhausted_worker_id,
                                    exhausted_account_id: downstream
                                        .worker_account_id(exhausted_worker_id),
                                    replacement_worker_id,
                                    replacement_account_id: downstream
                                        .worker_account_id(replacement_worker_id),
                                },
                            ),
                        );
                    }
                    info!(
                        method = request.method.as_str(),
                        tenant_id = context.tenant_id.as_str(),
                        project_id = context.project_id.as_deref(),
                        exhausted_worker_id,
                        exhausted_account_id = downstream.worker_account_id(exhausted_worker_id),
                        replacement_worker_id,
                        replacement_account_id =
                            downstream.worker_account_id(replacement_worker_id),
                        thread_path = request_thread_path(&request),
                        "gateway v2 restored a path-based thread request on another account-backed worker after account capacity exhaustion"
                    );
                }
                return Ok(Ok(apply_response_scope_policy(
                    scope_registry,
                    context,
                    Some(observability),
                    &request.method,
                    worker.worker_id,
                    worker
                        .worker_id
                        .and_then(|worker_id| downstream.worker_account_id(worker_id)),
                    result,
                )?));
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(worker_id) = exhausted_route_worker_id
        && let Some(thread_path) = request_thread_path(&request)
    {
        let message = format!(
            "thread path {thread_path} is pinned to worker {worker_id} with exhausted account capacity for {}, and no replacement worker restored the context",
            request.method
        );
        observability.record_v2_account_capacity_event(
            worker_id,
            "path_thread_handoff_failure",
            Some(context),
            Some(message.as_str()),
        );
        observability.publish_operator_event(GatewayEvent::account_path_handoff_failed(
            GatewayAccountPathHandoffFailed {
                tenant_id: context.tenant_id.as_str(),
                project_id: context.project_id.as_deref(),
                method: request.method.as_str(),
                thread_path,
                exhausted_worker_id: worker_id,
                exhausted_account_id: downstream.worker_account_id(worker_id),
                reason: message.as_str(),
            },
        ));
        warn!(
            method = request.method.as_str(),
            account_capacity_event = "path_thread_handoff_failure",
            tenant_id = context.tenant_id.as_str(),
            project_id = context.project_id.as_deref(),
            exhausted_worker_id = worker_id,
            exhausted_account_id = downstream.worker_account_id(worker_id),
            thread_path,
            "gateway v2 failed to restore a path-based thread request on another account-backed worker after account capacity exhaustion"
        );
        return Err(io::Error::other(FailClosedMultiWorkerRouteError {
            message,
        }));
    }

    match first_error {
        Some(error) => Ok(Err(error)),
        None => Err(io::Error::other(
            "gateway v2 connection has no downstream app-server sessions",
        )),
    }
}

pub(crate) fn thread_id_account_handoff_metrics(
    method: &str,
) -> Option<(&'static str, &'static str)> {
    match method {
        "getConversationSummary" => Some((
            "conversation_summary_handoff_success",
            "conversation_summary_handoff_failure",
        )),
        "thread/archive" => Some((
            "thread_archive_handoff_success",
            "thread_archive_handoff_failure",
        )),
        "thread/decrement_elicitation" => Some((
            "thread_decrement_elicitation_handoff_success",
            "thread_decrement_elicitation_handoff_failure",
        )),
        "thread/fork" => Some(("thread_fork_handoff_success", "thread_fork_handoff_failure")),
        "thread/increment_elicitation" => Some((
            "thread_increment_elicitation_handoff_success",
            "thread_increment_elicitation_handoff_failure",
        )),
        "thread/inject_items" => Some((
            "thread_inject_items_handoff_success",
            "thread_inject_items_handoff_failure",
        )),
        "thread/memoryMode/set" => Some((
            "thread_memory_mode_set_handoff_success",
            "thread_memory_mode_set_handoff_failure",
        )),
        "thread/metadata/update" => Some((
            "thread_metadata_update_handoff_success",
            "thread_metadata_update_handoff_failure",
        )),
        "thread/name/set" => Some((
            "thread_name_set_handoff_success",
            "thread_name_set_handoff_failure",
        )),
        "thread/read" => Some(("thread_read_handoff_success", "thread_read_handoff_failure")),
        "thread/resume" => Some((
            "thread_resume_handoff_success",
            "thread_resume_handoff_failure",
        )),
        "thread/rollback" => Some((
            "thread_rollback_handoff_success",
            "thread_rollback_handoff_failure",
        )),
        "thread/compact/start" => Some((
            "thread_compact_start_handoff_success",
            "thread_compact_start_handoff_failure",
        )),
        "thread/turns/list" => Some((
            "thread_turns_list_handoff_success",
            "thread_turns_list_handoff_failure",
        )),
        "thread/unsubscribe" => Some((
            "thread_unsubscribe_handoff_success",
            "thread_unsubscribe_handoff_failure",
        )),
        "thread/unarchive" => Some((
            "thread_unarchive_handoff_success",
            "thread_unarchive_handoff_failure",
        )),
        _ => None,
    }
}

pub(crate) fn is_account_capacity_error(error: &JSONRPCErrorError) -> bool {
    if error.code == 429 {
        return true;
    }

    let message = error.message.to_ascii_lowercase();
    [
        "rate limit",
        "rate_limit",
        "usage limit",
        "usage_limit",
        "credits depleted",
        "billing",
        "quota",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub(crate) fn is_multi_worker_fanout_login_request(request: &JSONRPCRequest) -> bool {
    request.method == "account/login/start"
        && request
            .params
            .as_ref()
            .and_then(|params| params.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|login_type| matches!(login_type, "apiKey" | "chatgptAuthTokens"))
}
