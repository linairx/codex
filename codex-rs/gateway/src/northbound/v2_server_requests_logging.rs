//! Logging helpers for northbound v2 server-request lifecycle handling.
//!
//! This module owns the structured logs and derived field summaries for
//! server-request cleanup and duplicate detection so the main server-request
//! module can focus on cleanup collection and metrics.

use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::RequestId;
use std::collections::HashMap;
use tracing::warn;

#[derive(Debug, Default)]
pub(crate) struct PendingServerRequestLogFields {
    pub(crate) gateway_request_ids: Vec<RequestId>,
    pub(crate) downstream_request_ids: Vec<RequestId>,
    pub(crate) methods: Vec<String>,
    pub(crate) thread_scoped_request_ids: Vec<RequestId>,
    pub(crate) connection_scoped_request_ids: Vec<RequestId>,
    pub(crate) thread_ids: Vec<String>,
    pub(crate) worker_ids: Vec<usize>,
    pub(crate) worker_websocket_urls: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ResolvedServerRequestLogFields {
    pub(crate) gateway_request_ids: Vec<RequestId>,
    pub(crate) downstream_request_ids: Vec<RequestId>,
    pub(crate) methods: Vec<String>,
    pub(crate) thread_ids: Vec<String>,
    pub(crate) worker_ids: Vec<usize>,
    pub(crate) worker_websocket_urls: Vec<String>,
}

pub(crate) fn pending_server_request_log_fields(
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
) -> PendingServerRequestLogFields {
    let mut gateway_request_ids = pending_server_requests.keys().cloned().collect::<Vec<_>>();
    gateway_request_ids.sort();

    let mut downstream_request_ids = pending_server_requests
        .values()
        .map(|route| route.downstream_request_id.clone())
        .collect::<Vec<_>>();
    downstream_request_ids.sort();

    let mut methods = pending_server_requests
        .values()
        .map(|route| route.method.clone())
        .collect::<Vec<_>>();
    methods.sort();
    methods.dedup();

    let mut thread_scoped_request_ids = pending_server_requests
        .iter()
        .filter(|(_, route)| route.thread_id.is_some())
        .map(|(request_id, _)| request_id.clone())
        .collect::<Vec<_>>();
    thread_scoped_request_ids.sort();

    let mut connection_scoped_request_ids = pending_server_requests
        .iter()
        .filter(|(_, route)| route.thread_id.is_none())
        .map(|(request_id, _)| request_id.clone())
        .collect::<Vec<_>>();
    connection_scoped_request_ids.sort();

    let mut thread_ids = pending_server_requests
        .values()
        .filter_map(|route| route.thread_id.clone())
        .collect::<Vec<_>>();
    thread_ids.sort();
    thread_ids.dedup();

    let mut worker_ids = pending_server_requests
        .values()
        .filter_map(|route| route.worker_id)
        .collect::<Vec<_>>();
    worker_ids.sort();
    worker_ids.dedup();

    let mut worker_websocket_urls = pending_server_requests
        .values()
        .map(|route| route.worker_websocket_url.clone())
        .collect::<Vec<_>>();
    worker_websocket_urls.sort();
    worker_websocket_urls.dedup();

    PendingServerRequestLogFields {
        gateway_request_ids,
        downstream_request_ids,
        methods,
        thread_scoped_request_ids,
        connection_scoped_request_ids,
        thread_ids,
        worker_ids,
        worker_websocket_urls,
    }
}

pub(crate) fn resolved_server_request_log_fields(
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) -> ResolvedServerRequestLogFields {
    let mut gateway_request_ids = resolved_server_requests
        .values()
        .map(|route| route.gateway_request_id.clone())
        .collect::<Vec<_>>();
    gateway_request_ids.sort();

    let mut downstream_request_ids = resolved_server_requests
        .keys()
        .map(|key| key.request_id.clone())
        .collect::<Vec<_>>();
    downstream_request_ids.sort();

    let mut methods = resolved_server_requests
        .values()
        .map(|route| route.method.clone())
        .collect::<Vec<_>>();
    methods.sort();
    methods.dedup();

    let mut worker_ids = resolved_server_requests
        .keys()
        .filter_map(|key| key.worker_id)
        .collect::<Vec<_>>();
    worker_ids.sort();
    worker_ids.dedup();

    let mut thread_ids = resolved_server_requests
        .values()
        .filter_map(|route| route.thread_id.clone())
        .collect::<Vec<_>>();
    thread_ids.sort();
    thread_ids.dedup();

    let mut worker_websocket_urls = resolved_server_requests
        .values()
        .map(|route| route.worker_websocket_url.clone())
        .collect::<Vec<_>>();
    worker_websocket_urls.sort();
    worker_websocket_urls.dedup();

    ResolvedServerRequestLogFields {
        gateway_request_ids,
        downstream_request_ids,
        methods,
        thread_ids,
        worker_ids,
        worker_websocket_urls,
    }
}

pub(crate) fn log_duplicate_downstream_server_request(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    request_id: &RequestId,
    method: &str,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
) {
    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        request_id = ?request_id,
        method,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        "closing gateway v2 connection because a downstream session reused a pending server-request id"
    );
}

pub(crate) fn log_dropped_duplicate_resolved_server_request(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    downstream_request_id: &RequestId,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) {
    let resolved_log_fields = resolved_server_request_log_fields(resolved_server_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        downstream_request_id = ?downstream_request_id,
        remaining_resolved_route_count = resolved_log_fields.gateway_request_ids.len(),
        remaining_resolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        remaining_resolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        remaining_resolved_server_request_methods = ?resolved_log_fields.methods,
        remaining_resolved_thread_ids = ?resolved_log_fields.thread_ids,
        remaining_resolved_worker_ids = ?resolved_log_fields.worker_ids,
        remaining_resolved_worker_websocket_urls = ?resolved_log_fields.worker_websocket_urls,
        "dropping duplicate downstream serverRequest/resolved replay after request-id translation"
    );
}

pub(crate) fn log_rejected_pending_server_requests(
    request_context: &GatewayRequestContext,
    connection_outcome: &str,
    connection_detail: Option<&str>,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
    pending_worker_websocket_urls: &[String],
) {
    if pending_server_requests.is_empty() && resolved_server_requests.is_empty() {
        return;
    }

    let pending_log_fields = pending_server_request_log_fields(pending_server_requests);
    let resolved_log_fields = resolved_server_request_log_fields(resolved_server_requests);
    let server_request_backlog_count = pending_log_fields
        .gateway_request_ids
        .len()
        .saturating_add(resolved_log_fields.gateway_request_ids.len());

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        connection_outcome,
        connection_detail,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        thread_scoped_pending_server_request_count = pending_log_fields.thread_scoped_request_ids.len(),
        connection_scoped_pending_server_request_count = pending_log_fields.connection_scoped_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        thread_scoped_pending_server_request_ids = ?pending_log_fields.thread_scoped_request_ids,
        connection_scoped_pending_server_request_ids = ?pending_log_fields.connection_scoped_request_ids,
        thread_ids = ?pending_log_fields.thread_ids,
        worker_ids = ?pending_log_fields.worker_ids,
        worker_websocket_urls = ?pending_worker_websocket_urls,
        answered_but_unresolved_server_request_count = resolved_log_fields.gateway_request_ids.len(),
        server_request_backlog_count,
        answered_but_unresolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        answered_but_unresolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        answered_but_unresolved_server_request_methods = ?resolved_log_fields.methods,
        answered_but_unresolved_thread_ids = ?resolved_log_fields.thread_ids,
        answered_but_unresolved_worker_ids = ?resolved_log_fields.worker_ids,
        answered_but_unresolved_worker_websocket_urls =
            ?resolved_log_fields.worker_websocket_urls,
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    );
}
