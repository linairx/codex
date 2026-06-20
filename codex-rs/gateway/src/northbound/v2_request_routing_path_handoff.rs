//! Path-based account-handoff helpers for northbound v2 client handling.
//!
//! This module owns rollout-path restoration so the thread-id handoff module can
//! stay focused on thread-id restoration and shared account-capacity helpers.

use crate::event::GatewayAccountPathHandoffFailed;
use crate::event::GatewayAccountPathHandoffSucceeded;
use crate::event::GatewayEvent;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_routing::path_handoff_response_must_match_request;
use crate::northbound::v2_routing::worker_for_server_request;
use crate::northbound::v2_scope::apply_response_scope_policy;
use crate::northbound::v2_scope::request_thread_path;
use crate::northbound::v2_scope::response_thread_path;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;
use std::io;
use tracing::info;
use tracing::warn;

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
