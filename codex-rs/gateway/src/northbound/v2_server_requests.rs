use crate::event::GatewayEvent;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
pub(crate) use crate::northbound::v2_server_requests_logging::log_dropped_duplicate_resolved_server_request;
pub(crate) use crate::northbound::v2_server_requests_logging::log_duplicate_downstream_server_request;
pub(crate) use crate::northbound::v2_server_requests_logging::pending_server_request_log_fields;
pub(crate) use crate::northbound::v2_server_requests_logging::resolved_server_request_log_fields;
use crate::observability::GatewayObservability;
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
