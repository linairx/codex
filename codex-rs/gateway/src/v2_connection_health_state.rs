use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayV2ConnectionPendingCounts {
    pub pending_client_request_count: usize,
    pub pending_client_request_worker_counts: Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub pending_client_request_method_counts: Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub pending_server_request_count: usize,
    pub answered_but_unresolved_server_request_count: usize,
    pub server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub server_request_backlog_method_counts: Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayV2ConnectionCompletionCounts {
    pub max_pending_client_request_count: usize,
    pub max_server_request_backlog_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct GatewayV2ConnectionHealthState {
    pub(super) next_connection_id: u64,
    pub(super) active_connection_count: usize,
    pub(super) active_connections: HashMap<u64, ActiveGatewayV2ConnectionHealth>,
    pub(super) account_capacity_event_counts: BTreeMap<String, usize>,
    pub(super) account_capacity_event_worker_counts: BTreeMap<usize, BTreeMap<String, usize>>,
    pub(super) last_account_capacity_event: Option<String>,
    pub(super) last_account_capacity_event_worker_id: Option<usize>,
    pub(super) last_account_capacity_event_tenant_id: Option<String>,
    pub(super) last_account_capacity_event_project_id: Option<String>,
    pub(super) last_account_capacity_event_reason: Option<String>,
    pub(super) last_account_capacity_event_at: Option<i64>,
    pub(super) worker_reconnect_event_counts: BTreeMap<String, usize>,
    pub(super) worker_reconnect_event_worker_counts: BTreeMap<usize, BTreeMap<String, usize>>,
    pub(super) last_worker_reconnect_event: Option<String>,
    pub(super) last_worker_reconnect_event_worker_id: Option<usize>,
    pub(super) last_worker_reconnect_event_at: Option<i64>,
    pub(super) request_counts: BTreeMap<(String, String), usize>,
    pub(super) last_request_method: Option<String>,
    pub(super) last_request_outcome: Option<String>,
    pub(super) last_request_duration_ms: Option<u64>,
    pub(super) max_request_duration_ms: Option<u64>,
    pub(super) last_request_at: Option<i64>,
    pub(super) client_request_rejection_counts: BTreeMap<(String, String), usize>,
    pub(super) last_client_request_rejection_method: Option<String>,
    pub(super) last_client_request_rejection_reason: Option<String>,
    pub(super) last_client_request_rejection_at: Option<i64>,
    pub(super) server_request_rejection_counts: BTreeMap<(String, String), usize>,
    pub(super) last_server_request_rejection_method: Option<String>,
    pub(super) last_server_request_rejection_reason: Option<String>,
    pub(super) last_server_request_rejection_at: Option<i64>,
    pub(super) server_request_lifecycle_event_counts: BTreeMap<(String, String), usize>,
    pub(super) last_server_request_lifecycle_event: Option<String>,
    pub(super) last_server_request_lifecycle_method: Option<String>,
    pub(super) last_server_request_lifecycle_at: Option<i64>,
    pub(super) fail_closed_request_counts: BTreeMap<(String, bool), usize>,
    pub(super) last_fail_closed_request_method: Option<String>,
    pub(super) last_fail_closed_request_reconnect_backoff_active: Option<bool>,
    pub(super) last_fail_closed_request_at: Option<i64>,
    pub(super) upstream_request_failure_counts: BTreeMap<(String, bool), usize>,
    pub(super) last_upstream_request_failure_method: Option<String>,
    pub(super) last_upstream_request_failure_reconnect_backoff_active: Option<bool>,
    pub(super) last_upstream_request_failure_at: Option<i64>,
    pub(super) downstream_backpressure_counts: BTreeMap<Option<usize>, usize>,
    pub(super) last_downstream_backpressure_worker_id: Option<usize>,
    pub(super) last_downstream_backpressure_at: Option<i64>,
    pub(super) client_send_timeout_count: usize,
    pub(super) last_client_send_timeout_at: Option<i64>,
    pub(super) thread_list_deduplication_counts: BTreeMap<Option<usize>, usize>,
    pub(super) last_thread_list_deduplication_selected_worker_id: Option<usize>,
    pub(super) last_thread_list_deduplication_at: Option<i64>,
    pub(super) thread_route_recovery_counts: BTreeMap<String, usize>,
    pub(super) last_thread_route_recovery_outcome: Option<String>,
    pub(super) last_thread_route_recovery_at: Option<i64>,
    pub(super) degraded_thread_discovery_counts: BTreeMap<(String, bool), usize>,
    pub(super) last_degraded_thread_discovery_method: Option<String>,
    pub(super) last_degraded_thread_discovery_reconnect_backoff_active: Option<bool>,
    pub(super) last_degraded_thread_discovery_at: Option<i64>,
    pub(super) project_worker_route_selection_count: usize,
    pub(super) project_worker_route_selection_worker_counts: BTreeMap<usize, usize>,
    pub(super) last_project_worker_route_selected_worker_id: Option<usize>,
    pub(super) last_project_worker_route_selected_tenant_id: Option<String>,
    pub(super) last_project_worker_route_selected_project_id: Option<String>,
    pub(super) last_project_worker_route_selected_thread_id: Option<String>,
    pub(super) last_project_worker_route_selected_account_id: Option<String>,
    pub(super) last_project_worker_route_selected_at: Option<i64>,
    pub(super) forwarded_notification_counts: BTreeMap<String, usize>,
    pub(super) last_forwarded_notification_method: Option<String>,
    pub(super) last_forwarded_notification_at: Option<i64>,
    pub(super) notification_send_failure_counts: BTreeMap<(String, String), usize>,
    pub(super) last_notification_send_failure_method: Option<String>,
    pub(super) last_notification_send_failure_outcome: Option<String>,
    pub(super) last_notification_send_failure_at: Option<i64>,
    pub(super) client_response_send_failure_counts: BTreeMap<(String, String), usize>,
    pub(super) last_client_response_send_failure_method: Option<String>,
    pub(super) last_client_response_send_failure_outcome: Option<String>,
    pub(super) last_client_response_send_failure_at: Option<i64>,
    pub(super) downstream_shutdown_failure_counts: BTreeMap<String, usize>,
    pub(super) last_downstream_shutdown_failure_outcome: Option<String>,
    pub(super) last_downstream_shutdown_failure_at: Option<i64>,
    pub(super) close_frame_send_failure_counts: BTreeMap<(u16, String), usize>,
    pub(super) last_close_frame_send_failure_code: Option<u16>,
    pub(super) last_close_frame_send_failure_outcome: Option<String>,
    pub(super) last_close_frame_send_failure_at: Option<i64>,
    pub(super) server_request_forward_send_failure_counts: BTreeMap<(String, String), usize>,
    pub(super) last_server_request_forward_send_failure_method: Option<String>,
    pub(super) last_server_request_forward_send_failure_outcome: Option<String>,
    pub(super) last_server_request_forward_send_failure_at: Option<i64>,
    pub(super) server_request_answer_delivery_failure_counts: BTreeMap<String, usize>,
    pub(super) last_server_request_answer_delivery_failure_response_kind: Option<String>,
    pub(super) last_server_request_answer_delivery_failure_at: Option<i64>,
    pub(super) server_request_rejection_delivery_failure_counts: BTreeMap<String, usize>,
    pub(super) last_server_request_rejection_delivery_failure_method: Option<String>,
    pub(super) last_server_request_rejection_delivery_failure_at: Option<i64>,
    pub(super) suppressed_notification_counts: BTreeMap<(String, String), usize>,
    pub(super) last_suppressed_notification_method: Option<String>,
    pub(super) last_suppressed_notification_reason: Option<String>,
    pub(super) last_suppressed_notification_at: Option<i64>,
    pub(super) protocol_violation_counts: BTreeMap<(String, String), usize>,
    pub(super) protocol_violation_worker_counts: BTreeMap<usize, BTreeMap<(String, String), usize>>,
    pub(super) last_protocol_violation_phase: Option<String>,
    pub(super) last_protocol_violation_reason: Option<String>,
    pub(super) last_protocol_violation_worker_id: Option<usize>,
    pub(super) last_protocol_violation_at: Option<i64>,
    pub(super) connection_outcome_counts: BTreeMap<String, usize>,
    pub(super) peak_active_connection_count: usize,
    pub(super) total_connection_count: u64,
    pub(super) last_connection_started_at: Option<i64>,
    pub(super) last_connection_completed_at: Option<i64>,
    pub(super) last_connection_duration_ms: Option<u64>,
    pub(super) max_connection_duration_ms: Option<u64>,
    pub(super) last_connection_outcome: Option<String>,
    pub(super) last_connection_detail: Option<String>,
    pub(super) last_connection_pending_client_request_count: usize,
    pub(super) last_connection_max_pending_client_request_count: usize,
    pub(super) last_connection_pending_client_request_started_at: Option<i64>,
    pub(super) last_connection_pending_client_request_worker_counts:
        Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub(super) last_connection_pending_client_request_method_counts:
        Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub(super) last_connection_pending_server_request_count: usize,
    pub(super) last_connection_answered_but_unresolved_server_request_count: usize,
    pub(super) last_connection_server_request_backlog_count: usize,
    pub(super) last_connection_max_server_request_backlog_count: usize,
    pub(super) last_connection_server_request_backlog_started_at: Option<i64>,
    pub(super) last_connection_server_request_backlog_worker_counts:
        Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub(super) last_connection_server_request_backlog_method_counts:
        Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ActiveGatewayV2ConnectionHealth {
    pub(super) pending_client_request_count: usize,
    pub(super) max_pending_client_request_count: usize,
    pub(super) pending_client_request_started_at: Option<i64>,
    pub(super) pending_client_request_worker_counts: Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub(super) pending_client_request_method_counts: Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub(super) pending_server_request_count: usize,
    pub(super) answered_but_unresolved_server_request_count: usize,
    pub(super) max_server_request_backlog_count: usize,
    pub(super) server_request_backlog_started_at: Option<i64>,
    pub(super) server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub(super) server_request_backlog_method_counts: Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

pub(super) fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(super) fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(super) fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
