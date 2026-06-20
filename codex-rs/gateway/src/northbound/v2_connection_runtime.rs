use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use crate::event::GatewayEvent;
use crate::northbound::v2_connection::DownstreamWorkerEvent;
use crate::northbound::v2_connection::DownstreamWorkerHandle;
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayRejectedServerRequest;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection_lifecycle::is_fail_closed_multi_worker_route_error;
use crate::northbound::v2_connection_lifecycle::reconnect_backoff_worker_routes;
use crate::northbound::v2_counts::answered_but_unresolved_server_request_count;
use crate::northbound::v2_counts::pending_client_request_method_counts;
use crate::northbound::v2_counts::pending_client_request_worker_counts;
use crate::northbound::v2_counts::server_request_backlog_method_counts;
use crate::northbound::v2_counts::server_request_backlog_worker_counts;
use crate::northbound::v2_routing::worker_for_server_request;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::v2::GatewayV2ConnectedSession;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use codex_app_server_client::AppServerEvent;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use std::collections::HashMap;
use std::io;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

pub(crate) fn update_active_v2_connection_pending_counts(
    observability: &GatewayObservability,
    connection_id: u64,
    event_state: &GatewayV2EventState,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) {
    observability
        .v2_connection_health()
        .update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: pending_client_requests.len(),
                pending_client_request_worker_counts: pending_client_request_worker_counts(
                    pending_client_requests,
                ),
                pending_client_request_method_counts: pending_client_request_method_counts(
                    pending_client_requests,
                ),
                pending_server_request_count: event_state.pending_server_requests.len(),
                answered_but_unresolved_server_request_count:
                    answered_but_unresolved_server_request_count(
                        &event_state.resolved_server_requests,
                    ),
                server_request_backlog_worker_counts: server_request_backlog_worker_counts(
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                ),
                server_request_backlog_method_counts: server_request_backlog_method_counts(
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                ),
            },
        );
}

pub(crate) struct GatewayV2ConnectionRunResult {
    pub(crate) outcome: &'static str,
    pub(crate) detail: Option<String>,
    pub(crate) pending_client_request_count: usize,
    pub(crate) pending_client_request_worker_counts: Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub(crate) pending_client_request_method_counts: Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub(crate) pending_server_request_count: usize,
    pub(crate) answered_but_unresolved_server_request_count: usize,
    pub(crate) server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub(crate) server_request_backlog_method_counts: Vec<GatewayV2ServerRequestBacklogMethodCounts>,
    pub(crate) result: io::Result<()>,
}

pub(crate) fn log_fail_closed_multi_worker_request(
    downstream: &GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    method: &str,
    err: &io::Error,
) {
    if !downstream.multi_worker_topology() {
        return;
    }

    let is_fail_closed_route_error = is_fail_closed_multi_worker_route_error(err);
    let unavailable_workers = downstream.unavailable_worker_route_diagnostics(Instant::now());
    if unavailable_workers.is_empty() && !is_fail_closed_route_error {
        return;
    }

    let available_worker_ids = downstream
        .workers
        .iter()
        .filter_map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let available_worker_websocket_urls = downstream
        .workers
        .iter()
        .filter_map(|worker| worker.worker_websocket_url.as_deref())
        .collect::<Vec<_>>();
    let unavailable_worker_ids = unavailable_workers
        .iter()
        .map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let unavailable_worker_websocket_urls = unavailable_workers
        .iter()
        .map(|worker| worker.websocket_url.as_str())
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_ids = unavailable_workers
        .iter()
        .filter(|worker| worker.reconnect_backoff_active)
        .map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_websocket_urls = unavailable_workers
        .iter()
        .filter(|worker| worker.reconnect_backoff_active)
        .map(|worker| worker.websocket_url.as_str())
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_remaining_seconds = unavailable_workers
        .iter()
        .filter_map(|worker| worker.reconnect_backoff_remaining_seconds)
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_routes = reconnect_backoff_worker_routes(&unavailable_workers);
    if is_fail_closed_route_error {
        connection
            .observability
            .record_v2_fail_closed_request(method, !reconnect_backoff_worker_ids.is_empty());

        warn!(
            method,
            tenant_id = connection.request_context.tenant_id.as_str(),
            project_id = connection.request_context.project_id.as_deref(),
            available_worker_ids = ?available_worker_ids,
            available_worker_websocket_urls = ?available_worker_websocket_urls,
            unavailable_worker_ids = ?unavailable_worker_ids,
            unavailable_worker_websocket_urls = ?unavailable_worker_websocket_urls,
            reconnect_backoff_worker_ids = ?reconnect_backoff_worker_ids,
            reconnect_backoff_worker_websocket_urls = ?reconnect_backoff_worker_websocket_urls,
            reconnect_backoff_worker_remaining_seconds = ?reconnect_backoff_worker_remaining_seconds,
            reconnect_backoff_worker_routes = ?reconnect_backoff_worker_routes,
            %err,
            "gateway v2 request failed closed because required worker routes are unavailable"
        );
    } else {
        connection
            .observability
            .record_v2_upstream_request_failure(method, !reconnect_backoff_worker_ids.is_empty());

        warn!(
            method,
            tenant_id = connection.request_context.tenant_id.as_str(),
            project_id = connection.request_context.project_id.as_deref(),
            available_worker_ids = ?available_worker_ids,
            available_worker_websocket_urls = ?available_worker_websocket_urls,
            unavailable_worker_ids = ?unavailable_worker_ids,
            unavailable_worker_websocket_urls = ?unavailable_worker_websocket_urls,
            reconnect_backoff_worker_ids = ?reconnect_backoff_worker_ids,
            reconnect_backoff_worker_websocket_urls = ?reconnect_backoff_worker_websocket_urls,
            reconnect_backoff_worker_remaining_seconds = ?reconnect_backoff_worker_remaining_seconds,
            reconnect_backoff_worker_routes = ?reconnect_backoff_worker_routes,
            %err,
            "gateway v2 upstream request failed while worker routes are unavailable"
        );
    }
}

pub(crate) fn log_degraded_multi_worker_thread_discovery(
    downstream: &GatewayV2DownstreamRouter,
    request_context: &GatewayRequestContext,
    observability: &GatewayObservability,
    method: &str,
) {
    if !downstream.multi_worker_topology() {
        return;
    }

    let unavailable_workers = downstream.unavailable_worker_route_diagnostics(Instant::now());
    if unavailable_workers.is_empty() {
        return;
    }

    let available_worker_ids = downstream
        .workers
        .iter()
        .filter_map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let available_worker_websocket_urls = downstream
        .workers
        .iter()
        .filter_map(|worker| worker.worker_websocket_url.as_deref())
        .collect::<Vec<_>>();
    let unavailable_worker_ids = unavailable_workers
        .iter()
        .map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let unavailable_worker_websocket_urls = unavailable_workers
        .iter()
        .map(|worker| worker.websocket_url.as_str())
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_ids = unavailable_workers
        .iter()
        .filter(|worker| worker.reconnect_backoff_active)
        .map(|worker| worker.worker_id)
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_websocket_urls = unavailable_workers
        .iter()
        .filter(|worker| worker.reconnect_backoff_active)
        .map(|worker| worker.websocket_url.as_str())
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_remaining_seconds = unavailable_workers
        .iter()
        .filter_map(|worker| worker.reconnect_backoff_remaining_seconds)
        .collect::<Vec<_>>();
    let reconnect_backoff_worker_routes = reconnect_backoff_worker_routes(&unavailable_workers);
    observability
        .record_v2_degraded_thread_discovery(method, !reconnect_backoff_worker_ids.is_empty());

    warn!(
        method,
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        available_worker_ids = ?available_worker_ids,
        available_worker_websocket_urls = ?available_worker_websocket_urls,
        unavailable_worker_ids = ?unavailable_worker_ids,
        unavailable_worker_websocket_urls = ?unavailable_worker_websocket_urls,
        reconnect_backoff_worker_ids = ?reconnect_backoff_worker_ids,
        reconnect_backoff_worker_websocket_urls = ?reconnect_backoff_worker_websocket_urls,
        reconnect_backoff_worker_remaining_seconds = ?reconnect_backoff_worker_remaining_seconds,
        reconnect_backoff_worker_routes = ?reconnect_backoff_worker_routes,
        "serving degraded multi-worker thread discovery from available workers"
    );
}

pub(crate) fn spawn_downstream_worker_session(
    event_tx: mpsc::Sender<DownstreamWorkerEvent>,
    session: GatewayV2ConnectedSession,
) -> (
    DownstreamWorkerHandle,
    tokio::sync::oneshot::Sender<()>,
    JoinHandle<io::Result<()>>,
) {
    let worker_id = session.worker_id;
    let worker_websocket_url = session.worker_websocket_url;
    let request_handle = session.app_server.request_handle();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let mut app_server = session.app_server;
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    break;
                }
                event = app_server.next_event() => {
                    let end_of_stream = event.is_none();
                    let terminal_disconnect = matches!(event, Some(AppServerEvent::Disconnected { .. }));
                    if event_tx
                        .send(DownstreamWorkerEvent { worker_id, event })
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if end_of_stream || terminal_disconnect {
                        break;
                    }
                }
            }
        }

        app_server.shutdown().await
    });
    (
        DownstreamWorkerHandle {
            worker_id,
            worker_websocket_url,
            request_handle,
        },
        shutdown_tx,
        event_task,
    )
}

pub(crate) async fn deliver_client_server_request_answer(
    downstream: &GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    gateway_request_id: RequestId,
    route: PendingServerRequestRoute,
    answer: crate::northbound::v2_connection::ClientServerRequestAnswer,
) -> io::Result<()> {
    let method_tag = answer.method_tag();
    let worker_id = route.worker_id;
    let worker_websocket_url = route.worker_websocket_url.clone();
    let downstream_request_id = route.downstream_request_id.clone();
    let thread_id = route.thread_id.clone();
    connection
        .observability
        .record_v2_server_request_lifecycle_event("client_server_request_answered", method_tag);
    let result = if downstream.multi_worker_topology()
        && let (Some(worker_id), Some(thread_id)) = (worker_id, thread_id.as_deref())
        && !downstream.worker_account_has_capacity(worker_id)
    {
        let message = format!(
            "thread {thread_id} is pinned to worker {worker_id} with exhausted account capacity for serverRequest/respond"
        );
        connection.observability.record_v2_account_capacity_event(
            worker_id,
            "active_thread_handoff_failure",
            Some(connection.request_context),
            Some(message.as_str()),
        );
        connection.observability.publish_operator_event(
            GatewayEvent::account_active_thread_handoff_failed(
                crate::event::GatewayAccountActiveThreadHandoffFailed {
                    tenant_id: connection.request_context.tenant_id.as_str(),
                    project_id: connection.request_context.project_id.as_deref(),
                    method: "serverRequest/respond",
                    thread_id,
                    exhausted_worker_id: worker_id,
                    exhausted_account_id: downstream.worker_account_id(worker_id),
                    reason: message.as_str(),
                },
            ),
        );
        warn!(
            response_kind = method_tag,
            account_capacity_event = "active_thread_handoff_failure",
            tenant_id = connection.request_context.tenant_id.as_str(),
            project_id = connection.request_context.project_id.as_deref(),
            thread_id,
            exhausted_worker_id = worker_id,
            exhausted_account_id = downstream.worker_account_id(worker_id),
            "gateway v2 failed closed for an answered server request pinned to an exhausted account because no protocol-visible context handoff was requested"
        );
        Err(io::Error::other(FailClosedMultiWorkerRouteError {
            message,
        }))
    } else {
        match worker_for_server_request(downstream, worker_id) {
            Ok(worker) => match answer {
                crate::northbound::v2_connection::ClientServerRequestAnswer::Response(result) => {
                    worker
                        .request_handle
                        .resolve_server_request(route.downstream_request_id, result)
                        .await
                }
                crate::northbound::v2_connection::ClientServerRequestAnswer::Error(error) => {
                    worker
                        .request_handle
                        .reject_server_request(route.downstream_request_id, error)
                        .await
                }
            },
            Err(err) => Err(err),
        }
    };
    let event = if result.is_ok() {
        "client_server_request_delivered"
    } else {
        "client_server_request_delivery_failed"
    };
    connection
        .observability
        .record_v2_server_request_lifecycle_event(event, method_tag);
    if let Err(err) = &result {
        connection
            .observability
            .record_v2_server_request_answer_delivery_failure(method_tag);
        warn!(
            tenant_id = connection.request_context.tenant_id.as_str(),
            project_id = connection.request_context.project_id.as_deref(),
            response_kind = method_tag,
            worker_id = ?worker_id,
            worker_websocket_url,
            gateway_request_id = ?gateway_request_id,
            downstream_request_id = ?downstream_request_id,
            thread_id = thread_id.as_deref(),
            error = %err,
            "failed to deliver answered server request back to downstream worker"
        );
    }
    result
}

pub(crate) async fn reject_downstream_server_request_at_gateway_boundary(
    downstream: &GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: GatewayRejectedServerRequest<'_>,
    error: JSONRPCErrorError,
) -> io::Result<()> {
    let downstream_request_id_for_log = request.downstream_request_id.clone();
    let result = match worker_for_server_request(downstream, request.worker_id) {
        Ok(worker) => {
            worker
                .request_handle
                .reject_server_request(request.downstream_request_id, error)
                .await
        }
        Err(err) => Err(err),
    };
    let event = if result.is_ok() {
        "downstream_server_request_rejection_delivered"
    } else {
        "downstream_server_request_rejection_delivery_failed"
    };
    connection
        .observability
        .record_v2_server_request_lifecycle_event(event, request.method);
    if let Err(err) = &result {
        connection
            .observability
            .record_v2_server_request_rejection_delivery_failure(request.method);
        warn!(
            tenant_id = connection.request_context.tenant_id.as_str(),
            project_id = connection.request_context.project_id.as_deref(),
            worker_id = ?request.worker_id,
            worker_websocket_url = request.worker_websocket_url,
            gateway_request_id = ?request.gateway_request_id,
            downstream_request_id = ?downstream_request_id_for_log,
            method = request.method,
            error = %err,
            "failed to deliver gateway rejected server request back to downstream worker"
        );
    }
    result
}
