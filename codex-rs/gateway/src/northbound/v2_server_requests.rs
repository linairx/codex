use crate::event::GatewayEvent;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequestResolvedNotification;
use std::collections::BTreeMap;
use std::collections::HashMap;
use tracing::warn;

pub(crate) fn collect_server_request_cleanup_for_worker(
    pending_server_requests: &mut HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &mut HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
    worker_id: Option<usize>,
) -> WorkerServerRequestCleanup {
    let mut cleanup = WorkerServerRequestCleanup::default();

    pending_server_requests.retain(|gateway_request_id, route| {
        if route.worker_id != worker_id {
            return true;
        }
        if let Some(thread_id) = route.thread_id.clone() {
            cleanup
                .resolved_notifications
                .push(WorkerCleanupResolvedNotification {
                    notification: ServerRequestResolvedNotification {
                        thread_id: thread_id.clone(),
                        request_id: gateway_request_id.clone(),
                    },
                    method: route.method.clone(),
                });
            cleanup.resolved_thread_scoped_requests += 1;
            cleanup
                .resolved_thread_scoped_request_ids
                .push(gateway_request_id.clone());
            cleanup
                .resolved_thread_scoped_downstream_request_ids
                .push(route.downstream_request_id.clone());
            cleanup
                .resolved_thread_scoped_methods
                .push(route.method.clone());
            cleanup.resolved_thread_scoped_thread_ids.push(thread_id);
        } else {
            cleanup.stranded_connection_scoped_requests += 1;
            cleanup
                .stranded_connection_scoped_request_ids
                .push(gateway_request_id.clone());
            cleanup
                .stranded_connection_scoped_downstream_request_ids
                .push(route.downstream_request_id.clone());
            cleanup
                .stranded_connection_scoped_methods
                .push(route.method.clone());
        }
        false
    });

    resolved_server_requests.retain(|key, route| {
        if key.worker_id != worker_id {
            return true;
        }
        if let Some(thread_id) = route.thread_id.clone() {
            cleanup
                .resolved_notifications
                .push(WorkerCleanupResolvedNotification {
                    notification: ServerRequestResolvedNotification {
                        thread_id: thread_id.clone(),
                        request_id: route.gateway_request_id.clone(),
                    },
                    method: route.method.clone(),
                });
            cleanup.resolved_thread_scoped_requests += 1;
            cleanup
                .resolved_thread_scoped_request_ids
                .push(route.gateway_request_id.clone());
            cleanup
                .resolved_thread_scoped_downstream_request_ids
                .push(key.request_id.clone());
            cleanup
                .resolved_thread_scoped_methods
                .push(route.method.clone());
            cleanup.resolved_thread_scoped_thread_ids.push(thread_id);
        } else {
            cleanup.stranded_connection_scoped_requests += 1;
            cleanup
                .stranded_connection_scoped_request_ids
                .push(route.gateway_request_id.clone());
            cleanup
                .stranded_connection_scoped_downstream_request_ids
                .push(key.request_id.clone());
            cleanup
                .stranded_connection_scoped_methods
                .push(route.method.clone());
        }
        false
    });

    cleanup
}

pub(crate) fn log_worker_server_request_cleanup(
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    remaining_worker_count: usize,
    disconnect_message: Option<&str>,
    cleanup: &WorkerServerRequestCleanup,
    message: &str,
) {
    let mut resolved_thread_scoped_request_ids = cleanup.resolved_thread_scoped_request_ids.clone();
    resolved_thread_scoped_request_ids.sort();
    let mut resolved_thread_scoped_downstream_request_ids = cleanup
        .resolved_thread_scoped_downstream_request_ids
        .clone();
    resolved_thread_scoped_downstream_request_ids.sort();
    let mut resolved_thread_scoped_methods = cleanup.resolved_thread_scoped_methods.clone();
    resolved_thread_scoped_methods.sort();
    resolved_thread_scoped_methods.dedup();
    let mut resolved_thread_scoped_thread_ids = cleanup.resolved_thread_scoped_thread_ids.clone();
    resolved_thread_scoped_thread_ids.sort();
    resolved_thread_scoped_thread_ids.dedup();
    let mut stranded_connection_scoped_request_ids =
        cleanup.stranded_connection_scoped_request_ids.clone();
    stranded_connection_scoped_request_ids.sort();
    let mut stranded_connection_scoped_downstream_request_ids = cleanup
        .stranded_connection_scoped_downstream_request_ids
        .clone();
    stranded_connection_scoped_downstream_request_ids.sort();
    let mut stranded_connection_scoped_methods = cleanup.stranded_connection_scoped_methods.clone();
    stranded_connection_scoped_methods.sort();
    stranded_connection_scoped_methods.dedup();

    warn!(
        worker_id = ?worker_id,
        worker_websocket_url,
        remaining_worker_count,
        resolved_thread_scoped_server_request_count = cleanup.resolved_thread_scoped_requests,
        resolved_thread_scoped_server_request_ids = ?resolved_thread_scoped_request_ids,
        resolved_thread_scoped_downstream_server_request_ids = ?resolved_thread_scoped_downstream_request_ids,
        resolved_thread_scoped_server_request_methods = ?resolved_thread_scoped_methods,
        resolved_thread_scoped_thread_ids = ?resolved_thread_scoped_thread_ids,
        stranded_connection_scoped_server_request_count = cleanup
            .stranded_connection_scoped_requests,
        stranded_connection_scoped_server_request_ids = ?stranded_connection_scoped_request_ids,
        stranded_connection_scoped_downstream_server_request_ids = ?stranded_connection_scoped_downstream_request_ids,
        stranded_connection_scoped_server_request_methods = ?stranded_connection_scoped_methods,
        disconnect_message,
        "{message}"
    );
}

pub(crate) fn publish_worker_server_request_cleanup_event(
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    remaining_worker_count: usize,
    disconnect_message: Option<&str>,
    cleanup: &WorkerServerRequestCleanup,
) {
    let mut resolved_thread_scoped_request_ids = cleanup.resolved_thread_scoped_request_ids.clone();
    resolved_thread_scoped_request_ids.sort();
    let mut resolved_thread_scoped_downstream_request_ids = cleanup
        .resolved_thread_scoped_downstream_request_ids
        .clone();
    resolved_thread_scoped_downstream_request_ids.sort();
    let mut resolved_thread_scoped_methods = cleanup.resolved_thread_scoped_methods.clone();
    resolved_thread_scoped_methods.sort();
    resolved_thread_scoped_methods.dedup();
    let mut resolved_thread_scoped_thread_ids = cleanup.resolved_thread_scoped_thread_ids.clone();
    resolved_thread_scoped_thread_ids.sort();
    resolved_thread_scoped_thread_ids.dedup();
    let mut stranded_connection_scoped_request_ids =
        cleanup.stranded_connection_scoped_request_ids.clone();
    stranded_connection_scoped_request_ids.sort();
    let mut stranded_connection_scoped_downstream_request_ids = cleanup
        .stranded_connection_scoped_downstream_request_ids
        .clone();
    stranded_connection_scoped_downstream_request_ids.sort();
    let mut stranded_connection_scoped_methods = cleanup.stranded_connection_scoped_methods.clone();
    stranded_connection_scoped_methods.sort();
    stranded_connection_scoped_methods.dedup();

    observability.publish_operator_event(GatewayEvent {
        method: "gateway/v2ServerRequestCleanup".to_string(),
        thread_id: None,
        data: serde_json::json!({
            "workerId": worker_id,
            "workerWebsocketUrl": worker_websocket_url,
            "remainingWorkerCount": remaining_worker_count,
            "disconnectMessage": disconnect_message,
            "resolvedThreadScopedServerRequestCount": cleanup.resolved_thread_scoped_requests,
            "resolvedThreadScopedServerRequestIds": resolved_thread_scoped_request_ids,
            "resolvedThreadScopedDownstreamServerRequestIds": resolved_thread_scoped_downstream_request_ids,
            "resolvedThreadScopedServerRequestMethods": resolved_thread_scoped_methods,
            "resolvedThreadScopedThreadIds": resolved_thread_scoped_thread_ids,
            "strandedConnectionScopedServerRequestCount": cleanup
                .stranded_connection_scoped_requests,
            "strandedConnectionScopedServerRequestIds": stranded_connection_scoped_request_ids,
            "strandedConnectionScopedDownstreamServerRequestIds": stranded_connection_scoped_downstream_request_ids,
            "strandedConnectionScopedServerRequestMethods": stranded_connection_scoped_methods,
        }),
    });
}

pub(crate) fn record_worker_server_request_cleanup_metrics(
    observability: &GatewayObservability,
    cleanup: &WorkerServerRequestCleanup,
) {
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "worker_cleanup_resolved_thread_scoped",
        &server_request_method_counts(&cleanup.resolved_thread_scoped_methods),
    );
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "worker_cleanup_stranded_connection_scoped",
        &server_request_method_counts(&cleanup.stranded_connection_scoped_methods),
    );
}

pub(crate) fn record_client_server_request_cleanup_metrics(
    observability: &GatewayObservability,
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) {
    let mut rejected_thread_scoped_requests = BTreeMap::new();
    let mut rejected_connection_scoped_requests = BTreeMap::new();
    for route in pending_server_requests.values() {
        let counts = if route.thread_id.is_some() {
            &mut rejected_thread_scoped_requests
        } else {
            &mut rejected_connection_scoped_requests
        };
        *counts.entry(route.method.clone()).or_insert(0) += 1;
    }
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_rejected_thread_scoped",
        &rejected_thread_scoped_requests,
    );
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_rejected_connection_scoped",
        &rejected_connection_scoped_requests,
    );
    let mut answered_but_unresolved_requests = BTreeMap::new();
    for route in resolved_server_requests.values() {
        *answered_but_unresolved_requests
            .entry(route.method.clone())
            .or_insert(0) += 1;
    }
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_answered_but_unresolved",
        &answered_but_unresolved_requests,
    );
}

pub(crate) fn record_v2_server_request_lifecycle_method_counts(
    observability: &GatewayObservability,
    event: &str,
    method_counts: &BTreeMap<String, i64>,
) {
    for (method, count) in method_counts {
        observability.record_v2_server_request_lifecycle_events(event, method, *count);
    }
}

fn server_request_method_counts(methods: &[String]) -> BTreeMap<String, i64> {
    let mut method_counts = BTreeMap::new();
    for method in methods {
        *method_counts.entry(method.clone()).or_insert(0) += 1;
    }
    method_counts
}

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

#[derive(Debug, Default)]
pub(crate) struct ResolvedServerRequestLogFields {
    pub(crate) gateway_request_ids: Vec<RequestId>,
    pub(crate) downstream_request_ids: Vec<RequestId>,
    pub(crate) methods: Vec<String>,
    pub(crate) thread_ids: Vec<String>,
    pub(crate) worker_ids: Vec<usize>,
    pub(crate) worker_websocket_urls: Vec<String>,
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
