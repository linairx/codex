//! Connection-lifecycle helpers for northbound v2 transport handling.
//!
//! This module owns logging and telemetry for pending request cleanup,
//! saturation handling, and connection shutdown so `v2.rs` can stay focused on
//! orchestration.

use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_connection::UnavailableWorkerRouteDiagnostics;
use crate::northbound::v2_counts::pending_client_request_method_counts;
use crate::northbound::v2_counts::pending_client_request_worker_counts;
use crate::northbound::v2_server_requests::pending_server_request_log_fields;
use crate::northbound::v2_server_requests::resolved_server_request_log_fields;
use crate::northbound::v2_wire::observe_v2_request;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use codex_app_server_protocol::RequestId;
use std::collections::HashMap;
use std::io;
use tracing::warn;

struct PendingClientRequestLogFields {
    count: usize,
    ids: Vec<RequestId>,
    methods: Vec<String>,
    worker_ids: Vec<usize>,
    worker_websocket_urls: Vec<String>,
}

fn pending_client_request_log_fields(
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) -> PendingClientRequestLogFields {
    let mut ids: Vec<RequestId> = pending_client_requests.keys().cloned().collect();
    ids.sort();
    let mut methods: Vec<String> = pending_client_requests
        .values()
        .map(|route| route.method.clone())
        .collect();
    methods.sort();
    let mut worker_ids: Vec<usize> = pending_client_requests
        .values()
        .filter_map(|route| route.worker_id)
        .collect();
    worker_ids.sort_unstable();
    let mut worker_websocket_urls: Vec<String> = pending_client_requests
        .values()
        .map(|route| route.worker_websocket_url.clone())
        .collect();
    worker_websocket_urls.sort();

    PendingClientRequestLogFields {
        count: ids.len(),
        ids,
        methods,
        worker_ids,
        worker_websocket_urls,
    }
}

pub(crate) fn log_unexpected_client_server_request_response(
    request_context: &GatewayRequestContext,
    response_kind: &str,
    unexpected_request_id: &RequestId,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) {
    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);
    let resolved_log_fields = resolved_server_request_log_fields(resolved_server_requests);
    let server_request_backlog_count = pending_log_fields
        .gateway_request_ids
        .len()
        .saturating_add(resolved_log_fields.gateway_request_ids.len());

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        response_kind,
        unexpected_request_id = ?unexpected_request_id,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        answered_but_unresolved_server_request_count = resolved_log_fields.gateway_request_ids.len(),
        server_request_backlog_count,
        answered_but_unresolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        answered_but_unresolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        answered_but_unresolved_server_request_methods = ?resolved_log_fields.methods,
        answered_but_unresolved_thread_ids = ?resolved_log_fields.thread_ids,
        answered_but_unresolved_worker_ids = ?resolved_log_fields.worker_ids,
        answered_but_unresolved_worker_websocket_urls =
            ?resolved_log_fields.worker_websocket_urls,
        "gateway v2 client replied to a server request that is no longer pending"
    );
}

pub(crate) fn log_rejected_saturated_server_request(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    request_id: &RequestId,
    method: &str,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    limit: usize,
) {
    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        pending_server_request_count = pending_server_requests.len(),
        limit,
        request_id = ?request_id,
        method,
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        "rejecting downstream server request because the gateway websocket connection is saturated"
    );
}

pub(crate) fn log_rejected_saturated_client_request(
    request_context: &GatewayRequestContext,
    request_id: &RequestId,
    method: &str,
    pending_client_request_count: usize,
    limit: usize,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        request_id = ?request_id,
        method,
        pending_client_request_count,
        limit,
        "rejecting client request because the gateway websocket connection is saturated"
    );
}

pub(crate) fn log_rejected_hidden_downstream_server_request(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    request_id: &RequestId,
    method: &str,
    thread_id: Option<&str>,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        request_id = ?request_id,
        method,
        thread_id,
        "rejecting downstream server request for a thread outside the gateway request scope"
    );
}

pub(crate) fn log_downstream_backpressure_close(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    skipped: usize,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) {
    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);
    let resolved_log_fields = resolved_server_request_log_fields(resolved_server_requests);
    let server_request_backlog_count = pending_log_fields
        .gateway_request_ids
        .len()
        .saturating_add(resolved_log_fields.gateway_request_ids.len());

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        skipped_event_count = skipped,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        answered_but_unresolved_server_request_count = resolved_log_fields.gateway_request_ids.len(),
        server_request_backlog_count,
        answered_but_unresolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        answered_but_unresolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        answered_but_unresolved_server_request_methods = ?resolved_log_fields.methods,
        answered_but_unresolved_thread_ids = ?resolved_log_fields.thread_ids,
        answered_but_unresolved_worker_ids = ?resolved_log_fields.worker_ids,
        answered_but_unresolved_worker_websocket_urls =
            ?resolved_log_fields.worker_websocket_urls,
        "closing gateway v2 connection because the downstream app-server event stream lagged"
    );
}

pub(crate) fn log_client_send_timeout(
    request_context: &GatewayRequestContext,
    detail: &str,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) {
    let pending_client_log_fields = pending_client_request_log_fields(pending_client_requests);
    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);
    let resolved_log_fields = resolved_server_request_log_fields(resolved_server_requests);
    let server_request_backlog_count = pending_log_fields
        .gateway_request_ids
        .len()
        .saturating_add(resolved_log_fields.gateway_request_ids.len());

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        connection_detail = detail,
        pending_client_request_count = pending_client_log_fields.count,
        pending_client_request_ids = ?pending_client_log_fields.ids,
        pending_client_request_methods = ?pending_client_log_fields.methods,
        pending_client_request_worker_ids = ?pending_client_log_fields.worker_ids,
        pending_client_request_worker_websocket_urls =
            ?pending_client_log_fields.worker_websocket_urls,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        answered_but_unresolved_server_request_count = resolved_log_fields.gateway_request_ids.len(),
        server_request_backlog_count,
        answered_but_unresolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        answered_but_unresolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        answered_but_unresolved_server_request_methods = ?resolved_log_fields.methods,
        answered_but_unresolved_thread_ids = ?resolved_log_fields.thread_ids,
        answered_but_unresolved_worker_ids = ?resolved_log_fields.worker_ids,
        answered_but_unresolved_worker_websocket_urls =
            ?resolved_log_fields.worker_websocket_urls,
        "closing gateway v2 connection because sending to the northbound client timed out"
    );
}

pub(crate) fn log_downstream_shutdown_failure(
    request_context: &GatewayRequestContext,
    connection_outcome: &str,
    connection_detail: Option<&str>,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
    pending_counts: &GatewayV2ConnectionPendingCounts,
    err: &io::Error,
) {
    let pending_client_log_fields = pending_client_request_log_fields(pending_client_requests);
    let pending_client_request_worker_counts = &pending_counts.pending_client_request_worker_counts;
    let pending_client_request_method_counts = &pending_counts.pending_client_request_method_counts;
    let server_request_backlog_count = pending_counts
        .pending_server_request_count
        .saturating_add(pending_counts.answered_but_unresolved_server_request_count);
    let server_request_backlog_worker_counts = &pending_counts.server_request_backlog_worker_counts;
    let server_request_backlog_method_counts = &pending_counts.server_request_backlog_method_counts;

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        connection_outcome,
        connection_detail,
        pending_client_request_count = pending_counts.pending_client_request_count,
        pending_client_request_ids = ?pending_client_log_fields.ids,
        pending_client_request_methods = ?pending_client_log_fields.methods,
        pending_client_request_worker_ids = ?pending_client_log_fields.worker_ids,
        pending_client_request_worker_websocket_urls =
            ?pending_client_log_fields.worker_websocket_urls,
        pending_client_request_worker_counts = ?pending_client_request_worker_counts,
        pending_client_request_method_counts = ?pending_client_request_method_counts,
        pending_server_request_count = pending_counts.pending_server_request_count,
        answered_but_unresolved_server_request_count =
            pending_counts.answered_but_unresolved_server_request_count,
        server_request_backlog_count,
        server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
        server_request_backlog_method_counts = ?server_request_backlog_method_counts,
        shutdown_error = %err,
        "gateway v2 websocket downstream shutdown also failed after connection error"
    );
}

pub(crate) fn log_aborted_pending_client_requests(
    request_context: &GatewayRequestContext,
    outcome: &str,
    detail: Option<&str>,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) {
    let pending_client_log_fields = pending_client_request_log_fields(pending_client_requests);
    let pending_client_request_worker_counts =
        pending_client_request_worker_counts(pending_client_requests);
    let pending_client_request_method_counts =
        pending_client_request_method_counts(pending_client_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        outcome,
        detail,
        pending_client_request_count = pending_client_log_fields.count,
        pending_client_request_ids = ?pending_client_log_fields.ids,
        pending_client_request_methods = ?pending_client_log_fields.methods,
        pending_client_request_worker_ids = ?pending_client_log_fields.worker_ids,
        pending_client_request_worker_websocket_urls =
            ?pending_client_log_fields.worker_websocket_urls,
        pending_client_request_worker_counts = ?pending_client_request_worker_counts,
        pending_client_request_method_counts = ?pending_client_request_method_counts,
        "aborting pending gateway v2 client requests because the northbound connection ended"
    );
}

pub(crate) fn log_duplicate_pending_client_request(
    request_context: &GatewayRequestContext,
    request_id: &RequestId,
    method: &str,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) {
    let pending_client_log_fields = pending_client_request_log_fields(pending_client_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        request_id = ?request_id,
        method,
        pending_client_request_count = pending_client_log_fields.count,
        pending_client_request_ids = ?pending_client_log_fields.ids,
        pending_client_request_methods = ?pending_client_log_fields.methods,
        pending_client_request_worker_ids = ?pending_client_log_fields.worker_ids,
        pending_client_request_worker_websocket_urls =
            ?pending_client_log_fields.worker_websocket_urls,
        "closing gateway v2 connection because the northbound client reused a pending request id"
    );
}

pub(crate) fn observe_aborted_pending_client_requests(
    observability: &GatewayObservability,
    outcome: &str,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) {
    for route in pending_client_requests.values() {
        observe_v2_request(
            observability,
            &route.request_context,
            &route.method,
            outcome,
            route.started_at.elapsed(),
        );
    }
}

pub(crate) fn is_fail_closed_multi_worker_route_error(err: &io::Error) -> bool {
    err.get_ref()
        .and_then(|source| {
            source
                .downcast_ref::<crate::northbound::v2_connection::FailClosedMultiWorkerRouteError>()
        })
        .is_some()
}

pub(crate) fn reconnect_backoff_worker_routes(
    unavailable_workers: &[UnavailableWorkerRouteDiagnostics],
) -> Vec<(usize, &str, u64)> {
    unavailable_workers
        .iter()
        .filter_map(|worker| {
            worker
                .reconnect_backoff_remaining_seconds
                .map(|remaining_seconds| {
                    (
                        worker.worker_id,
                        worker.websocket_url.as_str(),
                        remaining_seconds,
                    )
                })
        })
        .collect()
}
