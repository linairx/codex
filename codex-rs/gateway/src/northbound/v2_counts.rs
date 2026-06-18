use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use codex_app_server_protocol::RequestId;
use std::collections::BTreeMap;
use std::collections::HashMap;

pub(crate) fn answered_but_unresolved_server_request_count(
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) -> usize {
    resolved_server_requests.len()
}

pub(crate) fn pending_client_request_worker_counts(
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) -> Vec<GatewayV2PendingClientRequestWorkerCounts> {
    let mut worker_counts = BTreeMap::new();
    for route in pending_client_requests.values() {
        let entry = worker_counts.entry(route.worker_id).or_insert(0usize);
        *entry = entry.saturating_add(1);
    }
    worker_counts
        .into_iter()
        .map(
            |(worker_id, pending_count)| GatewayV2PendingClientRequestWorkerCounts {
                worker_id,
                pending_client_request_count: pending_count,
            },
        )
        .collect()
}

pub(crate) fn pending_client_request_method_counts(
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) -> Vec<GatewayV2PendingClientRequestMethodCounts> {
    let mut method_counts = BTreeMap::new();
    for route in pending_client_requests.values() {
        let entry = method_counts.entry(route.method.clone()).or_insert(0usize);
        *entry = entry.saturating_add(1);
    }
    method_counts
        .into_iter()
        .map(
            |(method, pending_count)| GatewayV2PendingClientRequestMethodCounts {
                method,
                pending_client_request_count: pending_count,
            },
        )
        .collect()
}

pub(crate) fn server_request_backlog_worker_counts(
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) -> Vec<GatewayV2ServerRequestBacklogWorkerCounts> {
    let mut worker_counts = BTreeMap::new();
    for route in pending_server_requests.values() {
        let entry = worker_counts
            .entry(route.worker_id)
            .or_insert((0usize, 0usize));
        entry.0 = entry.0.saturating_add(1);
    }
    for key in resolved_server_requests.keys() {
        let entry = worker_counts
            .entry(key.worker_id)
            .or_insert((0usize, 0usize));
        entry.1 = entry.1.saturating_add(1);
    }
    worker_counts
        .into_iter()
        .map(
            |(worker_id, (pending_count, answered_but_unresolved_count))| {
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id,
                    pending_server_request_count: pending_count,
                    answered_but_unresolved_server_request_count: answered_but_unresolved_count,
                    server_request_backlog_count: pending_count
                        .saturating_add(answered_but_unresolved_count),
                }
            },
        )
        .collect()
}

pub(crate) fn server_request_backlog_method_counts(
    pending_server_requests: &HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) -> Vec<GatewayV2ServerRequestBacklogMethodCounts> {
    let mut method_counts = BTreeMap::new();
    for route in pending_server_requests.values() {
        let entry = method_counts
            .entry(route.method.clone())
            .or_insert((0usize, 0usize));
        entry.0 = entry.0.saturating_add(1);
    }
    for route in resolved_server_requests.values() {
        let entry = method_counts
            .entry(route.method.clone())
            .or_insert((0usize, 0usize));
        entry.1 = entry.1.saturating_add(1);
    }
    method_counts
        .into_iter()
        .map(|(method, (pending_count, answered_but_unresolved_count))| {
            GatewayV2ServerRequestBacklogMethodCounts {
                method,
                pending_server_request_count: pending_count,
                answered_but_unresolved_server_request_count: answered_but_unresolved_count,
                server_request_backlog_count: pending_count
                    .saturating_add(answered_but_unresolved_count),
            }
        })
        .collect()
}
