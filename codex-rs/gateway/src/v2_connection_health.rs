use crate::api::GatewayV2AccountCapacityWorkerEventCounts;
use crate::api::GatewayV2ClientRequestRejectionCounts;
use crate::api::GatewayV2ClientResponseSendFailureCounts;
use crate::api::GatewayV2CloseFrameSendFailureCounts;
use crate::api::GatewayV2ConnectionHealth;
use crate::api::GatewayV2ConnectionOutcomeCounts;
use crate::api::GatewayV2DegradedThreadDiscoveryCounts;
use crate::api::GatewayV2DownstreamBackpressureCounts;
use crate::api::GatewayV2DownstreamShutdownFailureCounts;
use crate::api::GatewayV2FailClosedRequestCounts;
use crate::api::GatewayV2ForwardedNotificationCounts;
use crate::api::GatewayV2NotificationSendFailureCounts;
use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ProtocolViolationCounts;
use crate::api::GatewayV2ProtocolViolationWorkerCounts;
use crate::api::GatewayV2RequestCounts;
use crate::api::GatewayV2ServerRequestAnswerDeliveryFailureCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use crate::api::GatewayV2ServerRequestForwardSendFailureCounts;
use crate::api::GatewayV2ServerRequestLifecycleEventCounts;
use crate::api::GatewayV2ServerRequestRejectionCounts;
use crate::api::GatewayV2ServerRequestRejectionDeliveryFailureCounts;
use crate::api::GatewayV2SuppressedNotificationCounts;
use crate::api::GatewayV2ThreadListDeduplicationCounts;
use crate::api::GatewayV2ThreadRouteRecoveryCounts;
use crate::api::GatewayV2UpstreamRequestFailureCounts;
use crate::api::GatewayV2WorkerReconnectWorkerEventCounts;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::Duration;
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
struct GatewayV2ConnectionHealthState {
    next_connection_id: u64,
    active_connection_count: usize,
    active_connections: HashMap<u64, ActiveGatewayV2ConnectionHealth>,
    account_capacity_event_counts: BTreeMap<String, usize>,
    account_capacity_event_worker_counts: BTreeMap<usize, BTreeMap<String, usize>>,
    last_account_capacity_event: Option<String>,
    last_account_capacity_event_worker_id: Option<usize>,
    last_account_capacity_event_tenant_id: Option<String>,
    last_account_capacity_event_project_id: Option<String>,
    last_account_capacity_event_reason: Option<String>,
    last_account_capacity_event_at: Option<i64>,
    worker_reconnect_event_counts: BTreeMap<String, usize>,
    worker_reconnect_event_worker_counts: BTreeMap<usize, BTreeMap<String, usize>>,
    last_worker_reconnect_event: Option<String>,
    last_worker_reconnect_event_worker_id: Option<usize>,
    last_worker_reconnect_event_at: Option<i64>,
    request_counts: BTreeMap<(String, String), usize>,
    last_request_method: Option<String>,
    last_request_outcome: Option<String>,
    last_request_duration_ms: Option<u64>,
    max_request_duration_ms: Option<u64>,
    last_request_at: Option<i64>,
    client_request_rejection_counts: BTreeMap<(String, String), usize>,
    last_client_request_rejection_method: Option<String>,
    last_client_request_rejection_reason: Option<String>,
    last_client_request_rejection_at: Option<i64>,
    server_request_rejection_counts: BTreeMap<(String, String), usize>,
    last_server_request_rejection_method: Option<String>,
    last_server_request_rejection_reason: Option<String>,
    last_server_request_rejection_at: Option<i64>,
    server_request_lifecycle_event_counts: BTreeMap<(String, String), usize>,
    last_server_request_lifecycle_event: Option<String>,
    last_server_request_lifecycle_method: Option<String>,
    last_server_request_lifecycle_at: Option<i64>,
    fail_closed_request_counts: BTreeMap<(String, bool), usize>,
    last_fail_closed_request_method: Option<String>,
    last_fail_closed_request_reconnect_backoff_active: Option<bool>,
    last_fail_closed_request_at: Option<i64>,
    upstream_request_failure_counts: BTreeMap<(String, bool), usize>,
    last_upstream_request_failure_method: Option<String>,
    last_upstream_request_failure_reconnect_backoff_active: Option<bool>,
    last_upstream_request_failure_at: Option<i64>,
    downstream_backpressure_counts: BTreeMap<Option<usize>, usize>,
    last_downstream_backpressure_worker_id: Option<usize>,
    last_downstream_backpressure_at: Option<i64>,
    client_send_timeout_count: usize,
    last_client_send_timeout_at: Option<i64>,
    thread_list_deduplication_counts: BTreeMap<Option<usize>, usize>,
    last_thread_list_deduplication_selected_worker_id: Option<usize>,
    last_thread_list_deduplication_at: Option<i64>,
    thread_route_recovery_counts: BTreeMap<String, usize>,
    last_thread_route_recovery_outcome: Option<String>,
    last_thread_route_recovery_at: Option<i64>,
    degraded_thread_discovery_counts: BTreeMap<(String, bool), usize>,
    last_degraded_thread_discovery_method: Option<String>,
    last_degraded_thread_discovery_reconnect_backoff_active: Option<bool>,
    last_degraded_thread_discovery_at: Option<i64>,
    forwarded_notification_counts: BTreeMap<String, usize>,
    last_forwarded_notification_method: Option<String>,
    last_forwarded_notification_at: Option<i64>,
    notification_send_failure_counts: BTreeMap<(String, String), usize>,
    last_notification_send_failure_method: Option<String>,
    last_notification_send_failure_outcome: Option<String>,
    last_notification_send_failure_at: Option<i64>,
    client_response_send_failure_counts: BTreeMap<(String, String), usize>,
    last_client_response_send_failure_method: Option<String>,
    last_client_response_send_failure_outcome: Option<String>,
    last_client_response_send_failure_at: Option<i64>,
    downstream_shutdown_failure_counts: BTreeMap<String, usize>,
    last_downstream_shutdown_failure_outcome: Option<String>,
    last_downstream_shutdown_failure_at: Option<i64>,
    close_frame_send_failure_counts: BTreeMap<(u16, String), usize>,
    last_close_frame_send_failure_code: Option<u16>,
    last_close_frame_send_failure_outcome: Option<String>,
    last_close_frame_send_failure_at: Option<i64>,
    server_request_forward_send_failure_counts: BTreeMap<(String, String), usize>,
    last_server_request_forward_send_failure_method: Option<String>,
    last_server_request_forward_send_failure_outcome: Option<String>,
    last_server_request_forward_send_failure_at: Option<i64>,
    server_request_answer_delivery_failure_counts: BTreeMap<String, usize>,
    last_server_request_answer_delivery_failure_response_kind: Option<String>,
    last_server_request_answer_delivery_failure_at: Option<i64>,
    server_request_rejection_delivery_failure_counts: BTreeMap<String, usize>,
    last_server_request_rejection_delivery_failure_method: Option<String>,
    last_server_request_rejection_delivery_failure_at: Option<i64>,
    suppressed_notification_counts: BTreeMap<(String, String), usize>,
    last_suppressed_notification_method: Option<String>,
    last_suppressed_notification_reason: Option<String>,
    last_suppressed_notification_at: Option<i64>,
    protocol_violation_counts: BTreeMap<(String, String), usize>,
    protocol_violation_worker_counts: BTreeMap<usize, BTreeMap<(String, String), usize>>,
    last_protocol_violation_phase: Option<String>,
    last_protocol_violation_reason: Option<String>,
    last_protocol_violation_worker_id: Option<usize>,
    last_protocol_violation_at: Option<i64>,
    connection_outcome_counts: BTreeMap<String, usize>,
    peak_active_connection_count: usize,
    total_connection_count: u64,
    last_connection_started_at: Option<i64>,
    last_connection_completed_at: Option<i64>,
    last_connection_duration_ms: Option<u64>,
    max_connection_duration_ms: Option<u64>,
    last_connection_outcome: Option<String>,
    last_connection_detail: Option<String>,
    last_connection_pending_client_request_count: usize,
    last_connection_max_pending_client_request_count: usize,
    last_connection_pending_client_request_started_at: Option<i64>,
    last_connection_pending_client_request_worker_counts:
        Vec<GatewayV2PendingClientRequestWorkerCounts>,
    last_connection_pending_client_request_method_counts:
        Vec<GatewayV2PendingClientRequestMethodCounts>,
    last_connection_pending_server_request_count: usize,
    last_connection_answered_but_unresolved_server_request_count: usize,
    last_connection_server_request_backlog_count: usize,
    last_connection_max_server_request_backlog_count: usize,
    last_connection_server_request_backlog_started_at: Option<i64>,
    last_connection_server_request_backlog_worker_counts:
        Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    last_connection_server_request_backlog_method_counts:
        Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ActiveGatewayV2ConnectionHealth {
    pending_client_request_count: usize,
    max_pending_client_request_count: usize,
    pending_client_request_started_at: Option<i64>,
    pending_client_request_worker_counts: Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pending_client_request_method_counts: Vec<GatewayV2PendingClientRequestMethodCounts>,
    pending_server_request_count: usize,
    answered_but_unresolved_server_request_count: usize,
    max_server_request_backlog_count: usize,
    server_request_backlog_started_at: Option<i64>,
    server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    server_request_backlog_method_counts: Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

#[derive(Debug, Default)]
pub struct GatewayV2ConnectionHealthRegistry {
    state: RwLock<GatewayV2ConnectionHealthState>,
}

impl GatewayV2ConnectionHealthRegistry {
    pub fn mark_connection_started(&self) -> u64 {
        let mut state = write_guard(&self.state);
        let connection_id = state.next_connection_id;
        state.next_connection_id = state.next_connection_id.saturating_add(1);
        state
            .active_connections
            .insert(connection_id, ActiveGatewayV2ConnectionHealth::default());
        state.active_connection_count = state.active_connections.len();
        state.peak_active_connection_count = state
            .peak_active_connection_count
            .max(state.active_connection_count);
        state.total_connection_count = state.total_connection_count.saturating_add(1);
        state.last_connection_started_at = Some(unix_timestamp_now());
        connection_id
    }

    pub fn update_connection_pending_counts(
        &self,
        connection_id: u64,
        counts: GatewayV2ConnectionPendingCounts,
    ) {
        let mut state = write_guard(&self.state);
        if let Some(connection) = state.active_connections.get_mut(&connection_id) {
            if connection.pending_client_request_count == 0
                && counts.pending_client_request_count > 0
            {
                connection.pending_client_request_started_at = Some(unix_timestamp_now());
            } else if counts.pending_client_request_count == 0 {
                connection.pending_client_request_started_at = None;
            }
            connection.pending_client_request_count = counts.pending_client_request_count;
            connection.max_pending_client_request_count = connection
                .max_pending_client_request_count
                .max(counts.pending_client_request_count);
            connection.pending_client_request_worker_counts =
                counts.pending_client_request_worker_counts;
            connection.pending_client_request_method_counts =
                counts.pending_client_request_method_counts;
            let previous_server_request_backlog_count = connection
                .pending_server_request_count
                .saturating_add(connection.answered_but_unresolved_server_request_count);
            let server_request_backlog_count = counts
                .pending_server_request_count
                .saturating_add(counts.answered_but_unresolved_server_request_count);
            if previous_server_request_backlog_count == 0 && server_request_backlog_count > 0 {
                connection.server_request_backlog_started_at = Some(unix_timestamp_now());
            } else if server_request_backlog_count == 0 {
                connection.server_request_backlog_started_at = None;
            }
            connection.max_server_request_backlog_count = connection
                .max_server_request_backlog_count
                .max(server_request_backlog_count);
            connection.pending_server_request_count = counts.pending_server_request_count;
            connection.answered_but_unresolved_server_request_count =
                counts.answered_but_unresolved_server_request_count;
            connection.server_request_backlog_worker_counts =
                counts.server_request_backlog_worker_counts;
            connection.server_request_backlog_method_counts =
                counts.server_request_backlog_method_counts;
        }
    }

    pub fn record_account_capacity_event(
        &self,
        worker_id: usize,
        event: &str,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
        reason: Option<&str>,
    ) {
        let mut state = write_guard(&self.state);
        *state
            .account_capacity_event_counts
            .entry(event.to_string())
            .or_insert(0) += 1;
        *state
            .account_capacity_event_worker_counts
            .entry(worker_id)
            .or_default()
            .entry(event.to_string())
            .or_insert(0) += 1;
        state.last_account_capacity_event = Some(event.to_string());
        state.last_account_capacity_event_worker_id = Some(worker_id);
        state.last_account_capacity_event_tenant_id = tenant_id.map(ToString::to_string);
        state.last_account_capacity_event_project_id = project_id.map(ToString::to_string);
        state.last_account_capacity_event_reason = reason.map(ToString::to_string);
        state.last_account_capacity_event_at = Some(unix_timestamp_now());
    }

    pub fn record_worker_reconnect_event(&self, worker_id: usize, event: &str) {
        let mut state = write_guard(&self.state);
        *state
            .worker_reconnect_event_counts
            .entry(event.to_string())
            .or_insert(0) += 1;
        *state
            .worker_reconnect_event_worker_counts
            .entry(worker_id)
            .or_default()
            .entry(event.to_string())
            .or_insert(0) += 1;
        state.last_worker_reconnect_event = Some(event.to_string());
        state.last_worker_reconnect_event_worker_id = Some(worker_id);
        state.last_worker_reconnect_event_at = Some(unix_timestamp_now());
    }

    pub fn record_request(&self, method: &str, outcome: &str, duration: Duration) {
        let mut state = write_guard(&self.state);
        *state
            .request_counts
            .entry((method.to_string(), outcome.to_string()))
            .or_insert(0) += 1;
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        state.last_request_method = Some(method.to_string());
        state.last_request_outcome = Some(outcome.to_string());
        state.last_request_duration_ms = Some(duration_ms);
        state.max_request_duration_ms = Some(
            state
                .max_request_duration_ms
                .map_or(duration_ms, |max_duration_ms| {
                    max_duration_ms.max(duration_ms)
                }),
        );
        state.last_request_at = Some(unix_timestamp_now());
    }

    pub fn record_client_request_rejection(&self, method: &str, reason: &str) {
        let mut state = write_guard(&self.state);
        *state
            .client_request_rejection_counts
            .entry((method.to_string(), reason.to_string()))
            .or_insert(0) += 1;
        state.last_client_request_rejection_method = Some(method.to_string());
        state.last_client_request_rejection_reason = Some(reason.to_string());
        state.last_client_request_rejection_at = Some(unix_timestamp_now());
    }

    pub fn record_server_request_rejection(&self, method: &str, reason: &str) {
        let mut state = write_guard(&self.state);
        *state
            .server_request_rejection_counts
            .entry((method.to_string(), reason.to_string()))
            .or_insert(0) += 1;
        state.last_server_request_rejection_method = Some(method.to_string());
        state.last_server_request_rejection_reason = Some(reason.to_string());
        state.last_server_request_rejection_at = Some(unix_timestamp_now());
    }

    pub fn record_server_request_lifecycle_events(&self, event: &str, method: &str, count: usize) {
        if count == 0 {
            return;
        }

        let mut state = write_guard(&self.state);
        let entry = state
            .server_request_lifecycle_event_counts
            .entry((event.to_string(), method.to_string()))
            .or_insert(0);
        *entry = entry.saturating_add(count);
        state.last_server_request_lifecycle_event = Some(event.to_string());
        state.last_server_request_lifecycle_method = Some(method.to_string());
        state.last_server_request_lifecycle_at = Some(unix_timestamp_now());
    }

    pub fn record_fail_closed_request(&self, method: &str, reconnect_backoff_active: bool) {
        let mut state = write_guard(&self.state);
        *state
            .fail_closed_request_counts
            .entry((method.to_string(), reconnect_backoff_active))
            .or_insert(0) += 1;
        state.last_fail_closed_request_method = Some(method.to_string());
        state.last_fail_closed_request_reconnect_backoff_active = Some(reconnect_backoff_active);
        state.last_fail_closed_request_at = Some(unix_timestamp_now());
    }

    pub fn record_upstream_request_failure(&self, method: &str, reconnect_backoff_active: bool) {
        let mut state = write_guard(&self.state);
        *state
            .upstream_request_failure_counts
            .entry((method.to_string(), reconnect_backoff_active))
            .or_insert(0) += 1;
        state.last_upstream_request_failure_method = Some(method.to_string());
        state.last_upstream_request_failure_reconnect_backoff_active =
            Some(reconnect_backoff_active);
        state.last_upstream_request_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_downstream_backpressure(&self, worker_id: Option<usize>) {
        let mut state = write_guard(&self.state);
        *state
            .downstream_backpressure_counts
            .entry(worker_id)
            .or_insert(0) += 1;
        state.last_downstream_backpressure_worker_id = worker_id;
        state.last_downstream_backpressure_at = Some(unix_timestamp_now());
    }

    pub fn record_client_send_timeout(&self) {
        let mut state = write_guard(&self.state);
        state.client_send_timeout_count = state.client_send_timeout_count.saturating_add(1);
        state.last_client_send_timeout_at = Some(unix_timestamp_now());
    }

    pub fn record_thread_list_deduplication(&self, selected_worker_id: Option<usize>) {
        let mut state = write_guard(&self.state);
        *state
            .thread_list_deduplication_counts
            .entry(selected_worker_id)
            .or_insert(0) += 1;
        state.last_thread_list_deduplication_selected_worker_id = selected_worker_id;
        state.last_thread_list_deduplication_at = Some(unix_timestamp_now());
    }

    pub fn record_thread_route_recovery(&self, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .thread_route_recovery_counts
            .entry(outcome.to_string())
            .or_insert(0) += 1;
        state.last_thread_route_recovery_outcome = Some(outcome.to_string());
        state.last_thread_route_recovery_at = Some(unix_timestamp_now());
    }

    pub fn record_degraded_thread_discovery(&self, method: &str, reconnect_backoff_active: bool) {
        let mut state = write_guard(&self.state);
        *state
            .degraded_thread_discovery_counts
            .entry((method.to_string(), reconnect_backoff_active))
            .or_insert(0) += 1;
        state.last_degraded_thread_discovery_method = Some(method.to_string());
        state.last_degraded_thread_discovery_reconnect_backoff_active =
            Some(reconnect_backoff_active);
        state.last_degraded_thread_discovery_at = Some(unix_timestamp_now());
    }

    pub fn record_suppressed_notification(&self, method: &str, reason: &str) {
        let mut state = write_guard(&self.state);
        *state
            .suppressed_notification_counts
            .entry((method.to_string(), reason.to_string()))
            .or_insert(0) += 1;
        state.last_suppressed_notification_method = Some(method.to_string());
        state.last_suppressed_notification_reason = Some(reason.to_string());
        state.last_suppressed_notification_at = Some(unix_timestamp_now());
    }

    pub fn record_forwarded_notification(&self, method: &str) {
        let mut state = write_guard(&self.state);
        *state
            .forwarded_notification_counts
            .entry(method.to_string())
            .or_insert(0) += 1;
        state.last_forwarded_notification_method = Some(method.to_string());
        state.last_forwarded_notification_at = Some(unix_timestamp_now());
    }

    pub fn record_notification_send_failure(&self, method: &str, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .notification_send_failure_counts
            .entry((method.to_string(), outcome.to_string()))
            .or_insert(0) += 1;
        state.last_notification_send_failure_method = Some(method.to_string());
        state.last_notification_send_failure_outcome = Some(outcome.to_string());
        state.last_notification_send_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_client_response_send_failure(&self, method: &str, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .client_response_send_failure_counts
            .entry((method.to_string(), outcome.to_string()))
            .or_insert(0) += 1;
        state.last_client_response_send_failure_method = Some(method.to_string());
        state.last_client_response_send_failure_outcome = Some(outcome.to_string());
        state.last_client_response_send_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_downstream_shutdown_failure(&self, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .downstream_shutdown_failure_counts
            .entry(outcome.to_string())
            .or_insert(0) += 1;
        state.last_downstream_shutdown_failure_outcome = Some(outcome.to_string());
        state.last_downstream_shutdown_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_close_frame_send_failure(&self, code: u16, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .close_frame_send_failure_counts
            .entry((code, outcome.to_string()))
            .or_insert(0) += 1;
        state.last_close_frame_send_failure_code = Some(code);
        state.last_close_frame_send_failure_outcome = Some(outcome.to_string());
        state.last_close_frame_send_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_server_request_forward_send_failure(&self, method: &str, outcome: &str) {
        let mut state = write_guard(&self.state);
        *state
            .server_request_forward_send_failure_counts
            .entry((method.to_string(), outcome.to_string()))
            .or_insert(0) += 1;
        state.last_server_request_forward_send_failure_method = Some(method.to_string());
        state.last_server_request_forward_send_failure_outcome = Some(outcome.to_string());
        state.last_server_request_forward_send_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_server_request_answer_delivery_failure(&self, response_kind: &str) {
        let mut state = write_guard(&self.state);
        *state
            .server_request_answer_delivery_failure_counts
            .entry(response_kind.to_string())
            .or_insert(0) += 1;
        state.last_server_request_answer_delivery_failure_response_kind =
            Some(response_kind.to_string());
        state.last_server_request_answer_delivery_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_server_request_rejection_delivery_failure(&self, method: &str) {
        let mut state = write_guard(&self.state);
        *state
            .server_request_rejection_delivery_failure_counts
            .entry(method.to_string())
            .or_insert(0) += 1;
        state.last_server_request_rejection_delivery_failure_method = Some(method.to_string());
        state.last_server_request_rejection_delivery_failure_at = Some(unix_timestamp_now());
    }

    pub fn record_protocol_violation(&self, phase: &str, reason: &str) {
        self.record_protocol_violation_for_worker(phase, reason, None);
    }

    pub fn record_protocol_violation_for_worker(
        &self,
        phase: &str,
        reason: &str,
        worker_id: Option<usize>,
    ) {
        let mut state = write_guard(&self.state);
        *state
            .protocol_violation_counts
            .entry((phase.to_string(), reason.to_string()))
            .or_insert(0) += 1;
        if let Some(worker_id) = worker_id {
            *state
                .protocol_violation_worker_counts
                .entry(worker_id)
                .or_default()
                .entry((phase.to_string(), reason.to_string()))
                .or_insert(0) += 1;
        }
        state.last_protocol_violation_phase = Some(phase.to_string());
        state.last_protocol_violation_reason = Some(reason.to_string());
        state.last_protocol_violation_worker_id = worker_id;
        state.last_protocol_violation_at = Some(unix_timestamp_now());
    }

    pub fn mark_connection_completed(
        &self,
        connection_id: u64,
        outcome: &str,
        detail: Option<&str>,
        duration: Duration,
        counts: GatewayV2ConnectionPendingCounts,
    ) {
        let mut state = write_guard(&self.state);
        let completed_connection = state.active_connections.remove(&connection_id);
        state.active_connection_count = state.active_connections.len();
        let completed_at = unix_timestamp_now();
        *state
            .connection_outcome_counts
            .entry(outcome.to_string())
            .or_insert(0) += 1;
        state.last_connection_completed_at = Some(completed_at);
        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        state.last_connection_duration_ms = Some(duration_ms);
        state.max_connection_duration_ms = Some(
            state
                .max_connection_duration_ms
                .map_or(duration_ms, |max_duration_ms| {
                    max_duration_ms.max(duration_ms)
                }),
        );
        state.last_connection_outcome = Some(outcome.to_string());
        state.last_connection_detail = detail.map(ToString::to_string);
        state.last_connection_pending_client_request_count = counts.pending_client_request_count;
        state.last_connection_max_pending_client_request_count = completed_connection
            .as_ref()
            .map_or(counts.pending_client_request_count, |connection| {
                connection
                    .max_pending_client_request_count
                    .max(counts.pending_client_request_count)
            });
        let pending_client_request_started_at = completed_connection
            .as_ref()
            .and_then(|connection| connection.pending_client_request_started_at);
        state.last_connection_pending_client_request_started_at = pending_client_request_started_at
            .or({
                if counts.pending_client_request_count > 0 {
                    Some(completed_at)
                } else {
                    None
                }
            });
        state.last_connection_pending_client_request_worker_counts =
            counts.pending_client_request_worker_counts;
        state.last_connection_pending_client_request_method_counts =
            counts.pending_client_request_method_counts;
        state.last_connection_pending_server_request_count = counts.pending_server_request_count;
        state.last_connection_answered_but_unresolved_server_request_count =
            counts.answered_but_unresolved_server_request_count;
        state.last_connection_server_request_backlog_count = counts
            .pending_server_request_count
            .saturating_add(counts.answered_but_unresolved_server_request_count);
        state.last_connection_max_server_request_backlog_count =
            completed_connection.as_ref().map_or(
                state.last_connection_server_request_backlog_count,
                |connection| {
                    connection
                        .max_server_request_backlog_count
                        .max(state.last_connection_server_request_backlog_count)
                },
            );
        let server_request_backlog_started_at = completed_connection
            .and_then(|connection| connection.server_request_backlog_started_at);
        state.last_connection_server_request_backlog_started_at = server_request_backlog_started_at
            .or({
                if state.last_connection_server_request_backlog_count > 0 {
                    Some(completed_at)
                } else {
                    None
                }
            });
        state.last_connection_server_request_backlog_worker_counts =
            counts.server_request_backlog_worker_counts;
        state.last_connection_server_request_backlog_method_counts =
            counts.server_request_backlog_method_counts;
    }

    pub fn snapshot(&self) -> GatewayV2ConnectionHealth {
        let state = read_guard(&self.state);
        let active_connection_pending_client_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.pending_client_request_count)
            .sum();
        let active_connection_max_pending_client_request_count = state
            .active_connections
            .values()
            .map(|connection| connection.pending_client_request_count)
            .max()
            .unwrap_or(0);
        let active_connection_peak_pending_client_request_count = state
            .active_connections
            .values()
            .map(|connection| connection.max_pending_client_request_count)
            .max()
            .unwrap_or(0);
        let mut active_connection_pending_client_request_worker_counts = BTreeMap::new();
        let mut active_connection_pending_client_request_method_counts = BTreeMap::new();
        let active_connection_pending_client_request_started_at = state
            .active_connections
            .values()
            .filter_map(|connection| connection.pending_client_request_started_at)
            .min();
        for connection in state.active_connections.values() {
            for counts in &connection.pending_client_request_worker_counts {
                let entry = active_connection_pending_client_request_worker_counts
                    .entry(counts.worker_id)
                    .or_insert(0usize);
                *entry = entry.saturating_add(counts.pending_client_request_count);
            }
            for counts in &connection.pending_client_request_method_counts {
                let entry = active_connection_pending_client_request_method_counts
                    .entry(counts.method.clone())
                    .or_insert(0usize);
                *entry = entry.saturating_add(counts.pending_client_request_count);
            }
        }
        let active_connection_pending_client_request_worker_counts =
            active_connection_pending_client_request_worker_counts
                .into_iter()
                .map(
                    |(worker_id, pending_count)| GatewayV2PendingClientRequestWorkerCounts {
                        worker_id,
                        pending_client_request_count: pending_count,
                    },
                )
                .collect();
        let active_connection_pending_client_request_method_counts =
            active_connection_pending_client_request_method_counts
                .into_iter()
                .map(
                    |(method, pending_count)| GatewayV2PendingClientRequestMethodCounts {
                        method,
                        pending_client_request_count: pending_count,
                    },
                )
                .collect();
        let active_connection_pending_server_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.pending_server_request_count)
            .sum();
        let active_connection_answered_but_unresolved_server_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.answered_but_unresolved_server_request_count)
            .sum();
        let active_connection_server_request_backlog_started_at = state
            .active_connections
            .values()
            .filter_map(|connection| connection.server_request_backlog_started_at)
            .min();
        let active_connection_server_request_backlog_count =
            active_connection_pending_server_request_count
                .saturating_add(active_connection_answered_but_unresolved_server_request_count);
        let active_connection_max_server_request_backlog_count = state
            .active_connections
            .values()
            .map(|connection| {
                connection
                    .pending_server_request_count
                    .saturating_add(connection.answered_but_unresolved_server_request_count)
            })
            .max()
            .unwrap_or(0);
        let active_connection_peak_server_request_backlog_count = state
            .active_connections
            .values()
            .map(|connection| connection.max_server_request_backlog_count)
            .max()
            .unwrap_or(0);
        let mut active_connection_server_request_backlog_worker_counts = BTreeMap::new();
        let mut active_connection_server_request_backlog_method_counts = BTreeMap::new();
        for connection in state.active_connections.values() {
            for counts in &connection.server_request_backlog_worker_counts {
                let entry = active_connection_server_request_backlog_worker_counts
                    .entry(counts.worker_id)
                    .or_insert((0usize, 0usize));
                entry.0 = entry.0.saturating_add(counts.pending_server_request_count);
                entry.1 = entry
                    .1
                    .saturating_add(counts.answered_but_unresolved_server_request_count);
            }
            for counts in &connection.server_request_backlog_method_counts {
                let entry = active_connection_server_request_backlog_method_counts
                    .entry(counts.method.clone())
                    .or_insert((0usize, 0usize));
                entry.0 = entry.0.saturating_add(counts.pending_server_request_count);
                entry.1 = entry
                    .1
                    .saturating_add(counts.answered_but_unresolved_server_request_count);
            }
        }
        let active_connection_server_request_backlog_worker_counts =
            active_connection_server_request_backlog_worker_counts
                .into_iter()
                .map(
                    |(worker_id, (pending_count, answered_but_unresolved_count))| {
                        GatewayV2ServerRequestBacklogWorkerCounts {
                            worker_id,
                            pending_server_request_count: pending_count,
                            answered_but_unresolved_server_request_count:
                                answered_but_unresolved_count,
                            server_request_backlog_count: pending_count
                                .saturating_add(answered_but_unresolved_count),
                        }
                    },
                )
                .collect();
        let active_connection_server_request_backlog_method_counts =
            active_connection_server_request_backlog_method_counts
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
                .collect();
        GatewayV2ConnectionHealth {
            active_connection_count: state.active_connection_count,
            active_connection_pending_client_request_count,
            active_connection_max_pending_client_request_count,
            active_connection_peak_pending_client_request_count,
            active_connection_pending_client_request_started_at,
            active_connection_pending_client_request_worker_counts,
            active_connection_pending_client_request_method_counts,
            active_connection_pending_server_request_count,
            active_connection_answered_but_unresolved_server_request_count,
            active_connection_server_request_backlog_count,
            active_connection_max_server_request_backlog_count,
            active_connection_peak_server_request_backlog_count,
            active_connection_server_request_backlog_started_at,
            active_connection_server_request_backlog_worker_counts,
            active_connection_server_request_backlog_method_counts,
            account_capacity_event_counts: state.account_capacity_event_counts.clone(),
            account_capacity_event_worker_counts: state
                .account_capacity_event_worker_counts
                .iter()
                .map(
                    |(worker_id, event_counts)| GatewayV2AccountCapacityWorkerEventCounts {
                        worker_id: *worker_id,
                        event_counts: event_counts.clone(),
                    },
                )
                .collect(),
            last_account_capacity_event: state.last_account_capacity_event.clone(),
            last_account_capacity_event_worker_id: state.last_account_capacity_event_worker_id,
            last_account_capacity_event_tenant_id: state
                .last_account_capacity_event_tenant_id
                .clone(),
            last_account_capacity_event_project_id: state
                .last_account_capacity_event_project_id
                .clone(),
            last_account_capacity_event_reason: state.last_account_capacity_event_reason.clone(),
            last_account_capacity_event_at: state.last_account_capacity_event_at,
            worker_reconnect_event_counts: state.worker_reconnect_event_counts.clone(),
            worker_reconnect_event_worker_counts: state
                .worker_reconnect_event_worker_counts
                .iter()
                .map(
                    |(worker_id, event_counts)| GatewayV2WorkerReconnectWorkerEventCounts {
                        worker_id: *worker_id,
                        event_counts: event_counts.clone(),
                    },
                )
                .collect(),
            last_worker_reconnect_event: state.last_worker_reconnect_event.clone(),
            last_worker_reconnect_event_worker_id: state.last_worker_reconnect_event_worker_id,
            last_worker_reconnect_event_at: state.last_worker_reconnect_event_at,
            request_counts: state
                .request_counts
                .iter()
                .map(|((method, outcome), count)| GatewayV2RequestCounts {
                    method: method.clone(),
                    outcome: outcome.clone(),
                    count: *count,
                })
                .collect(),
            last_request_method: state.last_request_method.clone(),
            last_request_outcome: state.last_request_outcome.clone(),
            last_request_duration_ms: state.last_request_duration_ms,
            max_request_duration_ms: state.max_request_duration_ms,
            last_request_at: state.last_request_at,
            client_request_rejection_counts: state
                .client_request_rejection_counts
                .iter()
                .map(
                    |((method, reason), count)| GatewayV2ClientRequestRejectionCounts {
                        method: method.clone(),
                        reason: reason.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_client_request_rejection_method: state
                .last_client_request_rejection_method
                .clone(),
            last_client_request_rejection_reason: state
                .last_client_request_rejection_reason
                .clone(),
            last_client_request_rejection_at: state.last_client_request_rejection_at,
            server_request_rejection_counts: state
                .server_request_rejection_counts
                .iter()
                .map(
                    |((method, reason), count)| GatewayV2ServerRequestRejectionCounts {
                        method: method.clone(),
                        reason: reason.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_server_request_rejection_method: state
                .last_server_request_rejection_method
                .clone(),
            last_server_request_rejection_reason: state
                .last_server_request_rejection_reason
                .clone(),
            last_server_request_rejection_at: state.last_server_request_rejection_at,
            server_request_lifecycle_event_counts: state
                .server_request_lifecycle_event_counts
                .iter()
                .map(
                    |((event, method), count)| GatewayV2ServerRequestLifecycleEventCounts {
                        event: event.clone(),
                        method: method.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_server_request_lifecycle_event: state.last_server_request_lifecycle_event.clone(),
            last_server_request_lifecycle_method: state
                .last_server_request_lifecycle_method
                .clone(),
            last_server_request_lifecycle_at: state.last_server_request_lifecycle_at,
            fail_closed_request_counts: state
                .fail_closed_request_counts
                .iter()
                .map(|((method, reconnect_backoff_active), count)| {
                    GatewayV2FailClosedRequestCounts {
                        method: method.clone(),
                        reconnect_backoff_active: *reconnect_backoff_active,
                        count: *count,
                    }
                })
                .collect(),
            last_fail_closed_request_method: state.last_fail_closed_request_method.clone(),
            last_fail_closed_request_reconnect_backoff_active: state
                .last_fail_closed_request_reconnect_backoff_active,
            last_fail_closed_request_at: state.last_fail_closed_request_at,
            upstream_request_failure_counts: state
                .upstream_request_failure_counts
                .iter()
                .map(|((method, reconnect_backoff_active), count)| {
                    GatewayV2UpstreamRequestFailureCounts {
                        method: method.clone(),
                        reconnect_backoff_active: *reconnect_backoff_active,
                        count: *count,
                    }
                })
                .collect(),
            last_upstream_request_failure_method: state
                .last_upstream_request_failure_method
                .clone(),
            last_upstream_request_failure_reconnect_backoff_active: state
                .last_upstream_request_failure_reconnect_backoff_active,
            last_upstream_request_failure_at: state.last_upstream_request_failure_at,
            downstream_backpressure_counts: state
                .downstream_backpressure_counts
                .iter()
                .map(|(worker_id, count)| GatewayV2DownstreamBackpressureCounts {
                    worker_id: *worker_id,
                    count: *count,
                })
                .collect(),
            last_downstream_backpressure_worker_id: state.last_downstream_backpressure_worker_id,
            last_downstream_backpressure_at: state.last_downstream_backpressure_at,
            client_send_timeout_count: state.client_send_timeout_count,
            last_client_send_timeout_at: state.last_client_send_timeout_at,
            thread_list_deduplication_counts: state
                .thread_list_deduplication_counts
                .iter()
                .map(
                    |(selected_worker_id, count)| GatewayV2ThreadListDeduplicationCounts {
                        selected_worker_id: *selected_worker_id,
                        count: *count,
                    },
                )
                .collect(),
            last_thread_list_deduplication_selected_worker_id: state
                .last_thread_list_deduplication_selected_worker_id,
            last_thread_list_deduplication_at: state.last_thread_list_deduplication_at,
            thread_route_recovery_counts: state
                .thread_route_recovery_counts
                .iter()
                .map(|(outcome, count)| GatewayV2ThreadRouteRecoveryCounts {
                    outcome: outcome.clone(),
                    count: *count,
                })
                .collect(),
            last_thread_route_recovery_outcome: state.last_thread_route_recovery_outcome.clone(),
            last_thread_route_recovery_at: state.last_thread_route_recovery_at,
            degraded_thread_discovery_counts: state
                .degraded_thread_discovery_counts
                .iter()
                .map(|((method, reconnect_backoff_active), count)| {
                    GatewayV2DegradedThreadDiscoveryCounts {
                        method: method.clone(),
                        reconnect_backoff_active: *reconnect_backoff_active,
                        count: *count,
                    }
                })
                .collect(),
            last_degraded_thread_discovery_method: state
                .last_degraded_thread_discovery_method
                .clone(),
            last_degraded_thread_discovery_reconnect_backoff_active: state
                .last_degraded_thread_discovery_reconnect_backoff_active,
            last_degraded_thread_discovery_at: state.last_degraded_thread_discovery_at,
            forwarded_notification_counts: state
                .forwarded_notification_counts
                .iter()
                .map(|(method, count)| GatewayV2ForwardedNotificationCounts {
                    method: method.clone(),
                    count: *count,
                })
                .collect(),
            last_forwarded_notification_method: state.last_forwarded_notification_method.clone(),
            last_forwarded_notification_at: state.last_forwarded_notification_at,
            notification_send_failure_counts: state
                .notification_send_failure_counts
                .iter()
                .map(
                    |((method, outcome), count)| GatewayV2NotificationSendFailureCounts {
                        method: method.clone(),
                        outcome: outcome.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_notification_send_failure_method: state
                .last_notification_send_failure_method
                .clone(),
            last_notification_send_failure_outcome: state
                .last_notification_send_failure_outcome
                .clone(),
            last_notification_send_failure_at: state.last_notification_send_failure_at,
            client_response_send_failure_counts: state
                .client_response_send_failure_counts
                .iter()
                .map(
                    |((method, outcome), count)| GatewayV2ClientResponseSendFailureCounts {
                        method: method.clone(),
                        outcome: outcome.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_client_response_send_failure_method: state
                .last_client_response_send_failure_method
                .clone(),
            last_client_response_send_failure_outcome: state
                .last_client_response_send_failure_outcome
                .clone(),
            last_client_response_send_failure_at: state.last_client_response_send_failure_at,
            downstream_shutdown_failure_counts: state
                .downstream_shutdown_failure_counts
                .iter()
                .map(
                    |(outcome, count)| GatewayV2DownstreamShutdownFailureCounts {
                        outcome: outcome.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_downstream_shutdown_failure_outcome: state
                .last_downstream_shutdown_failure_outcome
                .clone(),
            last_downstream_shutdown_failure_at: state.last_downstream_shutdown_failure_at,
            close_frame_send_failure_counts: state
                .close_frame_send_failure_counts
                .iter()
                .map(
                    |((code, outcome), count)| GatewayV2CloseFrameSendFailureCounts {
                        code: *code,
                        outcome: outcome.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_close_frame_send_failure_code: state.last_close_frame_send_failure_code,
            last_close_frame_send_failure_outcome: state
                .last_close_frame_send_failure_outcome
                .clone(),
            last_close_frame_send_failure_at: state.last_close_frame_send_failure_at,
            server_request_forward_send_failure_counts: state
                .server_request_forward_send_failure_counts
                .iter()
                .map(
                    |((method, outcome), count)| GatewayV2ServerRequestForwardSendFailureCounts {
                        method: method.clone(),
                        outcome: outcome.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_server_request_forward_send_failure_method: state
                .last_server_request_forward_send_failure_method
                .clone(),
            last_server_request_forward_send_failure_outcome: state
                .last_server_request_forward_send_failure_outcome
                .clone(),
            last_server_request_forward_send_failure_at: state
                .last_server_request_forward_send_failure_at,
            server_request_answer_delivery_failure_counts: state
                .server_request_answer_delivery_failure_counts
                .iter()
                .map(
                    |(response_kind, count)| GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                        response_kind: response_kind.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_server_request_answer_delivery_failure_response_kind: state
                .last_server_request_answer_delivery_failure_response_kind
                .clone(),
            last_server_request_answer_delivery_failure_at: state
                .last_server_request_answer_delivery_failure_at,
            server_request_rejection_delivery_failure_counts: state
                .server_request_rejection_delivery_failure_counts
                .iter()
                .map(
                    |(method, count)| GatewayV2ServerRequestRejectionDeliveryFailureCounts {
                        method: method.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_server_request_rejection_delivery_failure_method: state
                .last_server_request_rejection_delivery_failure_method
                .clone(),
            last_server_request_rejection_delivery_failure_at: state
                .last_server_request_rejection_delivery_failure_at,
            suppressed_notification_counts: state
                .suppressed_notification_counts
                .iter()
                .map(
                    |((method, reason), count)| GatewayV2SuppressedNotificationCounts {
                        method: method.clone(),
                        reason: reason.clone(),
                        count: *count,
                    },
                )
                .collect(),
            last_suppressed_notification_method: state.last_suppressed_notification_method.clone(),
            last_suppressed_notification_reason: state.last_suppressed_notification_reason.clone(),
            last_suppressed_notification_at: state.last_suppressed_notification_at,
            protocol_violation_counts: state
                .protocol_violation_counts
                .iter()
                .map(
                    |((phase, reason), count)| GatewayV2ProtocolViolationCounts {
                        phase: phase.clone(),
                        reason: reason.clone(),
                        count: *count,
                    },
                )
                .collect(),
            protocol_violation_worker_counts: state
                .protocol_violation_worker_counts
                .iter()
                .map(
                    |(worker_id, violation_counts)| GatewayV2ProtocolViolationWorkerCounts {
                        worker_id: *worker_id,
                        violation_counts: violation_counts
                            .iter()
                            .map(
                                |((phase, reason), count)| GatewayV2ProtocolViolationCounts {
                                    phase: phase.clone(),
                                    reason: reason.clone(),
                                    count: *count,
                                },
                            )
                            .collect(),
                    },
                )
                .collect(),
            last_protocol_violation_phase: state.last_protocol_violation_phase.clone(),
            last_protocol_violation_reason: state.last_protocol_violation_reason.clone(),
            last_protocol_violation_worker_id: state.last_protocol_violation_worker_id,
            last_protocol_violation_at: state.last_protocol_violation_at,
            connection_outcome_counts: state
                .connection_outcome_counts
                .iter()
                .map(|(outcome, count)| GatewayV2ConnectionOutcomeCounts {
                    outcome: outcome.clone(),
                    count: *count,
                })
                .collect(),
            peak_active_connection_count: state.peak_active_connection_count,
            total_connection_count: state.total_connection_count,
            last_connection_started_at: state.last_connection_started_at,
            last_connection_completed_at: state.last_connection_completed_at,
            last_connection_duration_ms: state.last_connection_duration_ms,
            max_connection_duration_ms: state.max_connection_duration_ms,
            last_connection_outcome: state.last_connection_outcome.clone(),
            last_connection_detail: state.last_connection_detail.clone(),
            last_connection_pending_client_request_count: state
                .last_connection_pending_client_request_count,
            last_connection_max_pending_client_request_count: state
                .last_connection_max_pending_client_request_count,
            last_connection_pending_client_request_started_at: state
                .last_connection_pending_client_request_started_at,
            last_connection_pending_client_request_worker_counts: state
                .last_connection_pending_client_request_worker_counts
                .clone(),
            last_connection_pending_client_request_method_counts: state
                .last_connection_pending_client_request_method_counts
                .clone(),
            last_connection_pending_server_request_count: state
                .last_connection_pending_server_request_count,
            last_connection_answered_but_unresolved_server_request_count: state
                .last_connection_answered_but_unresolved_server_request_count,
            last_connection_server_request_backlog_count: state
                .last_connection_server_request_backlog_count,
            last_connection_max_server_request_backlog_count: state
                .last_connection_max_server_request_backlog_count,
            last_connection_server_request_backlog_started_at: state
                .last_connection_server_request_backlog_started_at,
            last_connection_server_request_backlog_worker_counts: state
                .last_connection_server_request_backlog_worker_counts
                .clone(),
            last_connection_server_request_backlog_method_counts: state
                .last_connection_server_request_backlog_method_counts
                .clone(),
        }
    }
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayV2ConnectionHealthRegistry;
    use super::GatewayV2ConnectionPendingCounts;
    use crate::api::GatewayV2ClientRequestRejectionCounts;
    use crate::api::GatewayV2ClientResponseSendFailureCounts;
    use crate::api::GatewayV2CloseFrameSendFailureCounts;
    use crate::api::GatewayV2ConnectionHealth;
    use crate::api::GatewayV2ConnectionOutcomeCounts;
    use crate::api::GatewayV2DegradedThreadDiscoveryCounts;
    use crate::api::GatewayV2DownstreamBackpressureCounts;
    use crate::api::GatewayV2DownstreamShutdownFailureCounts;
    use crate::api::GatewayV2FailClosedRequestCounts;
    use crate::api::GatewayV2ForwardedNotificationCounts;
    use crate::api::GatewayV2NotificationSendFailureCounts;
    use crate::api::GatewayV2PendingClientRequestMethodCounts;
    use crate::api::GatewayV2PendingClientRequestWorkerCounts;
    use crate::api::GatewayV2ProtocolViolationCounts;
    use crate::api::GatewayV2RequestCounts;
    use crate::api::GatewayV2ServerRequestAnswerDeliveryFailureCounts;
    use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
    use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
    use crate::api::GatewayV2ServerRequestForwardSendFailureCounts;
    use crate::api::GatewayV2ServerRequestLifecycleEventCounts;
    use crate::api::GatewayV2ServerRequestRejectionCounts;
    use crate::api::GatewayV2ServerRequestRejectionDeliveryFailureCounts;
    use crate::api::GatewayV2SuppressedNotificationCounts;
    use crate::api::GatewayV2ThreadListDeduplicationCounts;
    use crate::api::GatewayV2ThreadRouteRecoveryCounts;
    use crate::api::GatewayV2UpstreamRequestFailureCounts;
    use crate::api::GatewayV2WorkerReconnectWorkerEventCounts;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[test]
    fn snapshot_starts_empty() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        assert_eq!(registry.snapshot(), GatewayV2ConnectionHealth::default());
    }

    #[test]
    fn snapshot_tracks_active_and_last_completed_connection() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        let first_connection_id = registry.mark_connection_started();
        let second_connection_id = registry.mark_connection_started();
        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 7,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(1),
                        pending_client_request_count: 3,
                    },
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(2),
                        pending_client_request_count: 4,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 7,
                    },
                ],
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(1),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "serverRequest/elicitation".to_string(),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
            },
        );
        registry.update_connection_pending_counts(
            second_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 11,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: None,
                        pending_client_request_count: 1,
                    },
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(2),
                        pending_client_request_count: 10,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 8,
                    },
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "thread/read".to_string(),
                        pending_client_request_count: 3,
                    },
                ],
                pending_server_request_count: 5,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: None,
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 0,
                        server_request_backlog_count: 1,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 4,
                        server_request_backlog_count: 8,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 5,
                        answered_but_unresolved_server_request_count: 4,
                        server_request_backlog_count: 9,
                    },
                ],
            },
        );
        let active_snapshot = registry.snapshot();
        assert_eq!(active_snapshot.active_connection_count, 2);
        assert_eq!(
            active_snapshot.active_connection_pending_client_request_count,
            18
        );
        assert_eq!(
            active_snapshot.active_connection_max_pending_client_request_count,
            11
        );
        assert_eq!(
            active_snapshot.active_connection_peak_pending_client_request_count,
            11
        );
        assert_eq!(
            active_snapshot
                .active_connection_pending_client_request_started_at
                .is_some(),
            true
        );
        assert_eq!(
            active_snapshot.active_connection_pending_client_request_worker_counts,
            vec![
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: None,
                    pending_client_request_count: 1,
                },
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(1),
                    pending_client_request_count: 3,
                },
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(2),
                    pending_client_request_count: 14,
                },
            ]
        );
        assert_eq!(
            active_snapshot.active_connection_pending_client_request_method_counts,
            vec![
                GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 15,
                },
                GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 3,
                },
            ]
        );
        assert_eq!(
            active_snapshot.active_connection_pending_server_request_count,
            8
        );
        assert_eq!(
            active_snapshot.active_connection_answered_but_unresolved_server_request_count,
            6
        );
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_count,
            14
        );
        assert_eq!(
            active_snapshot.active_connection_max_server_request_backlog_count,
            9
        );
        assert_eq!(
            active_snapshot.active_connection_peak_server_request_backlog_count,
            9
        );
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_worker_counts,
            vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: None,
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(1),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 3,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(2),
                    pending_server_request_count: 5,
                    answered_but_unresolved_server_request_count: 5,
                    server_request_backlog_count: 10,
                },
            ]
        );
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_method_counts,
            vec![
                GatewayV2ServerRequestBacklogMethodCounts {
                    method: "item/tool/requestUserInput".to_string(),
                    pending_server_request_count: 7,
                    answered_but_unresolved_server_request_count: 5,
                    server_request_backlog_count: 12,
                },
                GatewayV2ServerRequestBacklogMethodCounts {
                    method: "serverRequest/elicitation".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 2,
                },
            ]
        );
        assert_eq!(
            active_snapshot
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
        assert_eq!(
            active_snapshot.account_capacity_event_counts,
            BTreeMap::new()
        );
        assert_eq!(active_snapshot.account_capacity_event_worker_counts, vec![]);
        assert_eq!(active_snapshot.last_account_capacity_event, None);
        assert_eq!(active_snapshot.last_account_capacity_event_worker_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_tenant_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_project_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_reason, None);
        assert_eq!(active_snapshot.last_account_capacity_event_at, None);
        assert_eq!(
            active_snapshot.worker_reconnect_event_counts,
            BTreeMap::new()
        );
        assert_eq!(active_snapshot.worker_reconnect_event_worker_counts, vec![]);
        assert_eq!(active_snapshot.last_worker_reconnect_event, None);
        assert_eq!(active_snapshot.last_worker_reconnect_event_worker_id, None);
        assert_eq!(active_snapshot.last_worker_reconnect_event_at, None);
        assert_eq!(active_snapshot.client_request_rejection_counts, vec![]);
        assert_eq!(active_snapshot.last_client_request_rejection_method, None);
        assert_eq!(active_snapshot.last_client_request_rejection_reason, None);
        assert_eq!(active_snapshot.last_client_request_rejection_at, None);
        assert_eq!(active_snapshot.server_request_rejection_counts, vec![]);
        assert_eq!(active_snapshot.last_server_request_rejection_method, None);
        assert_eq!(active_snapshot.last_server_request_rejection_reason, None);
        assert_eq!(active_snapshot.last_server_request_rejection_at, None);
        assert_eq!(
            active_snapshot.server_request_lifecycle_event_counts,
            vec![]
        );
        assert_eq!(active_snapshot.last_server_request_lifecycle_event, None);
        assert_eq!(active_snapshot.last_server_request_lifecycle_method, None);
        assert_eq!(active_snapshot.last_server_request_lifecycle_at, None);
        assert_eq!(active_snapshot.fail_closed_request_counts, vec![]);
        assert_eq!(active_snapshot.last_fail_closed_request_method, None);
        assert_eq!(
            active_snapshot.last_fail_closed_request_reconnect_backoff_active,
            None
        );
        assert_eq!(active_snapshot.last_fail_closed_request_at, None);
        assert_eq!(active_snapshot.upstream_request_failure_counts, vec![]);
        assert_eq!(active_snapshot.last_upstream_request_failure_method, None);
        assert_eq!(
            active_snapshot.last_upstream_request_failure_reconnect_backoff_active,
            None
        );
        assert_eq!(active_snapshot.last_upstream_request_failure_at, None);
        assert_eq!(active_snapshot.downstream_backpressure_counts, vec![]);
        assert_eq!(active_snapshot.last_downstream_backpressure_worker_id, None);
        assert_eq!(active_snapshot.last_downstream_backpressure_at, None);
        assert_eq!(active_snapshot.client_send_timeout_count, 0);
        assert_eq!(active_snapshot.last_client_send_timeout_at, None);
        assert_eq!(active_snapshot.thread_list_deduplication_counts, vec![]);
        assert_eq!(
            active_snapshot.last_thread_list_deduplication_selected_worker_id,
            None
        );
        assert_eq!(active_snapshot.last_thread_list_deduplication_at, None);
        assert_eq!(active_snapshot.thread_route_recovery_counts, vec![]);
        assert_eq!(active_snapshot.last_thread_route_recovery_outcome, None);
        assert_eq!(active_snapshot.last_thread_route_recovery_at, None);
        assert_eq!(active_snapshot.degraded_thread_discovery_counts, vec![]);
        assert_eq!(active_snapshot.last_degraded_thread_discovery_method, None);
        assert_eq!(
            active_snapshot.last_degraded_thread_discovery_reconnect_backoff_active,
            None
        );
        assert_eq!(active_snapshot.last_degraded_thread_discovery_at, None);
        assert_eq!(active_snapshot.forwarded_notification_counts, vec![]);
        assert_eq!(active_snapshot.last_forwarded_notification_method, None);
        assert_eq!(active_snapshot.last_forwarded_notification_at, None);
        assert_eq!(active_snapshot.notification_send_failure_counts, vec![]);
        assert_eq!(active_snapshot.last_notification_send_failure_method, None);
        assert_eq!(active_snapshot.last_notification_send_failure_outcome, None);
        assert_eq!(active_snapshot.last_notification_send_failure_at, None);
        assert_eq!(active_snapshot.client_response_send_failure_counts, vec![]);
        assert_eq!(
            active_snapshot.last_client_response_send_failure_method,
            None
        );
        assert_eq!(
            active_snapshot.last_client_response_send_failure_outcome,
            None
        );
        assert_eq!(active_snapshot.last_client_response_send_failure_at, None);
        assert_eq!(active_snapshot.downstream_shutdown_failure_counts, vec![]);
        assert_eq!(
            active_snapshot.last_downstream_shutdown_failure_outcome,
            None
        );
        assert_eq!(active_snapshot.last_downstream_shutdown_failure_at, None);
        assert_eq!(active_snapshot.close_frame_send_failure_counts, vec![]);
        assert_eq!(active_snapshot.last_close_frame_send_failure_code, None);
        assert_eq!(active_snapshot.last_close_frame_send_failure_outcome, None);
        assert_eq!(active_snapshot.last_close_frame_send_failure_at, None);
        assert_eq!(
            active_snapshot.server_request_forward_send_failure_counts,
            vec![]
        );
        assert_eq!(
            active_snapshot.last_server_request_forward_send_failure_method,
            None
        );
        assert_eq!(
            active_snapshot.last_server_request_forward_send_failure_outcome,
            None
        );
        assert_eq!(
            active_snapshot.last_server_request_forward_send_failure_at,
            None
        );
        assert_eq!(
            active_snapshot.server_request_answer_delivery_failure_counts,
            vec![]
        );
        assert_eq!(
            active_snapshot.last_server_request_answer_delivery_failure_response_kind,
            None
        );
        assert_eq!(
            active_snapshot.last_server_request_answer_delivery_failure_at,
            None
        );
        assert_eq!(
            active_snapshot.server_request_rejection_delivery_failure_counts,
            vec![]
        );
        assert_eq!(
            active_snapshot.last_server_request_rejection_delivery_failure_method,
            None
        );
        assert_eq!(
            active_snapshot.last_server_request_rejection_delivery_failure_at,
            None
        );
        assert_eq!(active_snapshot.suppressed_notification_counts, vec![]);
        assert_eq!(active_snapshot.last_suppressed_notification_method, None);
        assert_eq!(active_snapshot.last_suppressed_notification_reason, None);
        assert_eq!(active_snapshot.last_suppressed_notification_at, None);
        assert_eq!(active_snapshot.protocol_violation_counts, vec![]);
        assert_eq!(active_snapshot.protocol_violation_worker_counts, vec![]);
        assert_eq!(active_snapshot.last_protocol_violation_phase, None);
        assert_eq!(active_snapshot.last_protocol_violation_reason, None);
        assert_eq!(active_snapshot.last_protocol_violation_worker_id, None);
        assert_eq!(active_snapshot.last_protocol_violation_at, None);
        assert_eq!(active_snapshot.connection_outcome_counts, vec![]);
        assert_eq!(active_snapshot.peak_active_connection_count, 2);
        assert_eq!(active_snapshot.total_connection_count, 2);
        assert_eq!(active_snapshot.last_connection_started_at.is_some(), true);

        registry.mark_connection_completed(
            first_connection_id,
            "client_disconnected",
            Some("socket closed"),
            Duration::from_millis(42),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 7,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(1),
                        pending_client_request_count: 3,
                    },
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(2),
                        pending_client_request_count: 4,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 7,
                    },
                ],
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(1),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "serverRequest/elicitation".to_string(),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
            },
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_count, 1);
        assert_eq!(snapshot.active_connection_pending_client_request_count, 11);
        assert_eq!(
            snapshot.active_connection_max_pending_client_request_count,
            11
        );
        assert_eq!(
            snapshot.active_connection_peak_pending_client_request_count,
            11
        );
        assert_eq!(
            snapshot
                .active_connection_pending_client_request_started_at
                .is_some(),
            true
        );
        assert_eq!(snapshot.active_connection_pending_server_request_count, 5);
        assert_eq!(
            snapshot.active_connection_answered_but_unresolved_server_request_count,
            4
        );
        assert_eq!(snapshot.active_connection_server_request_backlog_count, 9);
        assert_eq!(
            snapshot.active_connection_max_server_request_backlog_count,
            9
        );
        assert_eq!(
            snapshot.active_connection_peak_server_request_backlog_count,
            9
        );
        assert_eq!(snapshot.peak_active_connection_count, 2);
        assert_eq!(snapshot.total_connection_count, 2);
        assert_eq!(
            snapshot.last_connection_outcome,
            Some("client_disconnected".to_string())
        );
        assert_eq!(
            snapshot.last_connection_detail,
            Some("socket closed".to_string())
        );
        assert_eq!(
            snapshot.connection_outcome_counts,
            vec![GatewayV2ConnectionOutcomeCounts {
                outcome: "client_disconnected".to_string(),
                count: 1,
            }]
        );
        assert_eq!(snapshot.last_connection_duration_ms, Some(42));
        assert_eq!(snapshot.max_connection_duration_ms, Some(42));
        assert_eq!(snapshot.last_connection_pending_client_request_count, 7);
        assert_eq!(snapshot.last_connection_max_pending_client_request_count, 7);
        assert_eq!(
            snapshot
                .last_connection_pending_client_request_started_at
                .is_some(),
            true
        );
        assert_eq!(
            snapshot.last_connection_pending_client_request_worker_counts,
            vec![
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(1),
                    pending_client_request_count: 3,
                },
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(2),
                    pending_client_request_count: 4,
                },
            ]
        );
        assert_eq!(
            snapshot.last_connection_pending_client_request_method_counts,
            vec![GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 7,
            }]
        );
        assert_eq!(snapshot.last_connection_pending_server_request_count, 3);
        assert_eq!(
            snapshot.last_connection_answered_but_unresolved_server_request_count,
            2
        );
        assert_eq!(snapshot.last_connection_server_request_backlog_count, 5);
        assert_eq!(snapshot.last_connection_max_server_request_backlog_count, 5);
        assert_eq!(
            snapshot.last_connection_server_request_backlog_worker_counts,
            vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(1),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 3,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(2),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.last_connection_server_request_backlog_method_counts,
            vec![
                GatewayV2ServerRequestBacklogMethodCounts {
                    method: "item/tool/requestUserInput".to_string(),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 3,
                },
                GatewayV2ServerRequestBacklogMethodCounts {
                    method: "serverRequest/elicitation".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot
                .last_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
        assert_eq!(snapshot.last_connection_started_at.is_some(), true);
        assert_eq!(snapshot.last_connection_completed_at.is_some(), true);
    }

    #[test]
    fn server_request_backlog_timestamp_resets_when_backlog_clears() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts::default(),
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at,
            None
        );

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
    }

    #[test]
    fn active_server_request_backlog_timestamp_survives_when_another_connection_clears() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let first_connection_id = registry.mark_connection_started();
        let second_connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        registry.update_connection_pending_counts(
            second_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts::default(),
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_pending_server_request_count, 1);
        assert_eq!(
            snapshot.active_connection_answered_but_unresolved_server_request_count,
            4
        );
        assert_eq!(snapshot.active_connection_server_request_backlog_count, 5);
        assert_eq!(
            snapshot.active_connection_max_server_request_backlog_count,
            5
        );
        assert_eq!(
            snapshot.active_connection_peak_server_request_backlog_count,
            5
        );
        assert_eq!(
            snapshot
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
    }

    #[test]
    fn pending_client_request_timestamp_resets_when_queue_clears() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 1,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_pending_client_request_started_at
                .is_some(),
            true
        );

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts::default(),
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_pending_client_request_started_at,
            None
        );
    }

    #[test]
    fn active_pending_client_timestamp_survives_when_another_connection_clears() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let first_connection_id = registry.mark_connection_started();
        let second_connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 3,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        registry.update_connection_pending_counts(
            second_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 5,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts::default(),
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_pending_client_request_count, 5);
        assert_eq!(
            snapshot.active_connection_max_pending_client_request_count,
            5
        );
        assert_eq!(
            snapshot.active_connection_peak_pending_client_request_count,
            5
        );
        assert_eq!(
            snapshot
                .active_connection_pending_client_request_started_at
                .is_some(),
            true
        );
    }

    #[test]
    fn completion_backfills_backlog_timestamp_when_counts_arrive_at_teardown() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.mark_connection_completed(
            connection_id,
            "client_disconnected",
            None,
            Duration::from_millis(1),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.last_connection_server_request_backlog_count, 2);
        assert_eq!(snapshot.last_connection_max_server_request_backlog_count, 2);
        assert_eq!(
            snapshot
                .last_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
    }

    #[test]
    fn completion_backfills_pending_client_timestamp_when_counts_arrive_at_teardown() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.mark_connection_completed(
            connection_id,
            "client_send_timed_out",
            None,
            Duration::from_millis(1),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 2,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.last_connection_pending_client_request_count, 2);
        assert_eq!(snapshot.last_connection_max_pending_client_request_count, 2);
        assert_eq!(
            snapshot
                .last_connection_pending_client_request_started_at
                .is_some(),
            true
        );
    }

    #[test]
    fn pending_client_request_peak_survives_queue_clear_and_completion() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 2,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        let active_snapshot = registry.snapshot();
        assert_eq!(
            active_snapshot.active_connection_pending_client_request_count,
            2
        );
        assert_eq!(
            active_snapshot.active_connection_max_pending_client_request_count,
            2
        );
        assert_eq!(
            active_snapshot.active_connection_peak_pending_client_request_count,
            6
        );

        registry.mark_connection_completed(
            connection_id,
            "client_disconnected",
            None,
            Duration::from_millis(1),
            GatewayV2ConnectionPendingCounts::default(),
        );

        let completed_snapshot = registry.snapshot();
        assert_eq!(
            completed_snapshot.last_connection_pending_client_request_count,
            0
        );
        assert_eq!(
            completed_snapshot.last_connection_max_pending_client_request_count,
            6
        );
    }

    #[test]
    fn server_request_backlog_peak_survives_queue_clear_and_completion() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 4,
                answered_but_unresolved_server_request_count: 3,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );
        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

        let active_snapshot = registry.snapshot();
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_count,
            3
        );
        assert_eq!(
            active_snapshot.active_connection_max_server_request_backlog_count,
            3
        );
        assert_eq!(
            active_snapshot.active_connection_peak_server_request_backlog_count,
            7
        );

        registry.mark_connection_completed(
            connection_id,
            "client_disconnected",
            None,
            Duration::from_millis(1),
            GatewayV2ConnectionPendingCounts::default(),
        );

        let completed_snapshot = registry.snapshot();
        assert_eq!(
            completed_snapshot.last_connection_server_request_backlog_count,
            0
        );
        assert_eq!(
            completed_snapshot.last_connection_max_server_request_backlog_count,
            7
        );
    }

    #[test]
    fn snapshot_clamps_last_completed_connection_duration() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        let connection_id = registry.mark_connection_started();
        registry.mark_connection_completed(
            connection_id,
            "client_disconnected",
            None,
            Duration::from_secs(u64::MAX),
            GatewayV2ConnectionPendingCounts::default(),
        );

        assert_eq!(
            registry.snapshot().last_connection_duration_ms,
            Some(u64::MAX)
        );
        assert_eq!(
            registry.snapshot().max_connection_duration_ms,
            Some(u64::MAX)
        );
    }

    #[test]
    fn snapshot_tracks_completed_connection_outcomes_and_max_duration() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        let first_connection_id = registry.mark_connection_started();
        registry.mark_connection_completed(
            first_connection_id,
            "normal",
            None,
            Duration::from_millis(12),
            GatewayV2ConnectionPendingCounts::default(),
        );
        let second_connection_id = registry.mark_connection_started();
        registry.mark_connection_completed(
            second_connection_id,
            "client_send_timed_out",
            None,
            Duration::from_millis(30),
            GatewayV2ConnectionPendingCounts::default(),
        );
        let third_connection_id = registry.mark_connection_started();
        registry.mark_connection_completed(
            third_connection_id,
            "normal",
            None,
            Duration::from_millis(5),
            GatewayV2ConnectionPendingCounts::default(),
        );

        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot.connection_outcome_counts,
            vec![
                GatewayV2ConnectionOutcomeCounts {
                    outcome: "client_send_timed_out".to_string(),
                    count: 1,
                },
                GatewayV2ConnectionOutcomeCounts {
                    outcome: "normal".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(snapshot.last_connection_duration_ms, Some(5));
        assert_eq!(snapshot.max_connection_duration_ms, Some(30));
        assert_eq!(snapshot.last_connection_outcome, Some("normal".to_string()));
    }

    #[test]
    fn snapshot_tracks_account_capacity_events() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_account_capacity_event(
            1,
            "exhausted",
            Some("tenant-a"),
            Some("project-a"),
            Some("quota exhausted"),
        );
        registry.record_account_capacity_event(
            2,
            "thread_read_handoff_success",
            Some("tenant-a"),
            Some("project-a"),
            Some("restored"),
        );
        registry.record_account_capacity_event(
            3,
            "exhausted",
            Some("tenant-b"),
            None,
            Some("billing limit"),
        );

        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot.account_capacity_event_counts,
            BTreeMap::from([
                ("exhausted".to_string(), 2),
                ("thread_read_handoff_success".to_string(), 1),
            ])
        );
        assert_eq!(
            snapshot.account_capacity_event_worker_counts,
            vec![
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 1,
                    event_counts: BTreeMap::from([("exhausted".to_string(), 1)]),
                },
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 2,
                    event_counts: BTreeMap::from([("thread_read_handoff_success".to_string(), 1)]),
                },
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 3,
                    event_counts: BTreeMap::from([("exhausted".to_string(), 1)]),
                },
            ]
        );
        assert_eq!(
            snapshot.last_account_capacity_event,
            Some("exhausted".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_worker_id, Some(3));
        assert_eq!(
            snapshot.last_account_capacity_event_tenant_id,
            Some("tenant-b".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_project_id, None);
        assert_eq!(
            snapshot.last_account_capacity_event_reason,
            Some("billing limit".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_worker_reconnect_events() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_worker_reconnect_event(1, "attempt");
        registry.record_worker_reconnect_event(2, "success");
        registry.record_worker_reconnect_event(1, "connect_failure");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.worker_reconnect_event_counts,
            BTreeMap::from([
                ("attempt".to_string(), 1),
                ("connect_failure".to_string(), 1),
                ("success".to_string(), 1),
            ])
        );
        assert_eq!(
            snapshot.worker_reconnect_event_worker_counts,
            vec![
                GatewayV2WorkerReconnectWorkerEventCounts {
                    worker_id: 1,
                    event_counts: BTreeMap::from([
                        ("attempt".to_string(), 1),
                        ("connect_failure".to_string(), 1),
                    ]),
                },
                GatewayV2WorkerReconnectWorkerEventCounts {
                    worker_id: 2,
                    event_counts: BTreeMap::from([("success".to_string(), 1)]),
                },
            ]
        );
        assert_eq!(
            snapshot.last_worker_reconnect_event,
            Some("connect_failure".to_string())
        );
        assert_eq!(snapshot.last_worker_reconnect_event_worker_id, Some(1));
        assert_eq!(snapshot.last_worker_reconnect_event_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_requests() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_request("initialize", "ok", Duration::from_millis(5));
        registry.record_request("thread/start", "ok", Duration::from_millis(10));
        registry.record_request("thread/start", "ok", Duration::from_millis(25));
        registry.record_request("turn/start", "rate_limited", Duration::from_millis(8));

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.request_counts,
            vec![
                GatewayV2RequestCounts {
                    method: "initialize".to_string(),
                    outcome: "ok".to_string(),
                    count: 1,
                },
                GatewayV2RequestCounts {
                    method: "thread/start".to_string(),
                    outcome: "ok".to_string(),
                    count: 2,
                },
                GatewayV2RequestCounts {
                    method: "turn/start".to_string(),
                    outcome: "rate_limited".to_string(),
                    count: 1,
                },
            ]
        );
        assert_eq!(snapshot.last_request_method, Some("turn/start".to_string()));
        assert_eq!(
            snapshot.last_request_outcome,
            Some("rate_limited".to_string())
        );
        assert_eq!(snapshot.last_request_duration_ms, Some(8));
        assert_eq!(snapshot.max_request_duration_ms, Some(25));
        assert_eq!(snapshot.last_request_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_request_rejections() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_client_request_rejection("command/exec", "pending_limit");
        registry.record_client_request_rejection("command/exec", "pending_limit");
        registry.record_client_request_rejection("turn/start", "rate_limited");
        registry.record_server_request_rejection("item/tool/requestUserInput", "pending_limit");
        registry.record_server_request_rejection("item/tool/requestUserInput", "pending_limit");
        registry
            .record_server_request_rejection("item/permissions/requestApproval", "hidden_thread");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.client_request_rejection_counts,
            vec![
                GatewayV2ClientRequestRejectionCounts {
                    method: "command/exec".to_string(),
                    reason: "pending_limit".to_string(),
                    count: 2,
                },
                GatewayV2ClientRequestRejectionCounts {
                    method: "turn/start".to_string(),
                    reason: "rate_limited".to_string(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_client_request_rejection_method,
            Some("turn/start".to_string())
        );
        assert_eq!(
            snapshot.last_client_request_rejection_reason,
            Some("rate_limited".to_string())
        );
        assert_eq!(snapshot.last_client_request_rejection_at.is_some(), true);
        assert_eq!(
            snapshot.server_request_rejection_counts,
            vec![
                GatewayV2ServerRequestRejectionCounts {
                    method: "item/permissions/requestApproval".to_string(),
                    reason: "hidden_thread".to_string(),
                    count: 1,
                },
                GatewayV2ServerRequestRejectionCounts {
                    method: "item/tool/requestUserInput".to_string(),
                    reason: "pending_limit".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.last_server_request_rejection_method,
            Some("item/permissions/requestApproval".to_string())
        );
        assert_eq!(
            snapshot.last_server_request_rejection_reason,
            Some("hidden_thread".to_string())
        );
        assert_eq!(snapshot.last_server_request_rejection_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_server_request_lifecycle_events() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_server_request_lifecycle_events(
            "client_server_request_answered",
            "response",
            1,
        );
        registry.record_server_request_lifecycle_events(
            "client_server_request_delivered",
            "response",
            2,
        );
        registry.record_server_request_lifecycle_events(
            "client_server_request_delivered",
            "response",
            3,
        );
        registry.record_server_request_lifecycle_events(
            "client_server_request_delivered",
            "response",
            0,
        );

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.server_request_lifecycle_event_counts,
            vec![
                GatewayV2ServerRequestLifecycleEventCounts {
                    event: "client_server_request_answered".to_string(),
                    method: "response".to_string(),
                    count: 1,
                },
                GatewayV2ServerRequestLifecycleEventCounts {
                    event: "client_server_request_delivered".to_string(),
                    method: "response".to_string(),
                    count: 5,
                },
            ]
        );
        assert_eq!(
            snapshot.last_server_request_lifecycle_event,
            Some("client_server_request_delivered".to_string())
        );
        assert_eq!(
            snapshot.last_server_request_lifecycle_method,
            Some("response".to_string())
        );
        assert_eq!(snapshot.last_server_request_lifecycle_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_fail_closed_requests() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_fail_closed_request("config/read", true);
        registry.record_fail_closed_request("config/read", true);
        registry.record_fail_closed_request("turn/start", false);

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.fail_closed_request_counts,
            vec![
                GatewayV2FailClosedRequestCounts {
                    method: "config/read".to_string(),
                    reconnect_backoff_active: true,
                    count: 2,
                },
                GatewayV2FailClosedRequestCounts {
                    method: "turn/start".to_string(),
                    reconnect_backoff_active: false,
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_fail_closed_request_method,
            Some("turn/start".to_string())
        );
        assert_eq!(
            snapshot.last_fail_closed_request_reconnect_backoff_active,
            Some(false)
        );
        assert_eq!(snapshot.last_fail_closed_request_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_upstream_request_failures() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_upstream_request_failure("config/read", true);
        registry.record_upstream_request_failure("config/read", true);
        registry.record_upstream_request_failure("thread/read", false);

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.upstream_request_failure_counts,
            vec![
                GatewayV2UpstreamRequestFailureCounts {
                    method: "config/read".to_string(),
                    reconnect_backoff_active: true,
                    count: 2,
                },
                GatewayV2UpstreamRequestFailureCounts {
                    method: "thread/read".to_string(),
                    reconnect_backoff_active: false,
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_upstream_request_failure_method,
            Some("thread/read".to_string())
        );
        assert_eq!(
            snapshot.last_upstream_request_failure_reconnect_backoff_active,
            Some(false)
        );
        assert_eq!(snapshot.last_upstream_request_failure_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_downstream_backpressure_and_client_send_timeouts() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_downstream_backpressure(None);
        registry.record_downstream_backpressure(Some(3));
        registry.record_downstream_backpressure(Some(3));
        registry.record_client_send_timeout();
        registry.record_client_send_timeout();

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.downstream_backpressure_counts,
            vec![
                GatewayV2DownstreamBackpressureCounts {
                    worker_id: None,
                    count: 1,
                },
                GatewayV2DownstreamBackpressureCounts {
                    worker_id: Some(3),
                    count: 2,
                },
            ]
        );
        assert_eq!(snapshot.last_downstream_backpressure_worker_id, Some(3));
        assert_eq!(snapshot.last_downstream_backpressure_at.is_some(), true);
        assert_eq!(snapshot.client_send_timeout_count, 2);
        assert_eq!(snapshot.last_client_send_timeout_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_thread_routing_diagnostics() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_thread_list_deduplication(None);
        registry.record_thread_list_deduplication(Some(3));
        registry.record_thread_list_deduplication(Some(3));
        registry.record_thread_route_recovery("miss");
        registry.record_thread_route_recovery("success");
        registry.record_thread_route_recovery("success");
        registry.record_degraded_thread_discovery("thread/list", true);
        registry.record_degraded_thread_discovery("thread/list", true);
        registry.record_degraded_thread_discovery("thread/loaded/list", false);

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.thread_list_deduplication_counts,
            vec![
                GatewayV2ThreadListDeduplicationCounts {
                    selected_worker_id: None,
                    count: 1,
                },
                GatewayV2ThreadListDeduplicationCounts {
                    selected_worker_id: Some(3),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.last_thread_list_deduplication_selected_worker_id,
            Some(3)
        );
        assert_eq!(snapshot.last_thread_list_deduplication_at.is_some(), true);
        assert_eq!(
            snapshot.thread_route_recovery_counts,
            vec![
                GatewayV2ThreadRouteRecoveryCounts {
                    outcome: "miss".to_string(),
                    count: 1,
                },
                GatewayV2ThreadRouteRecoveryCounts {
                    outcome: "success".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.last_thread_route_recovery_outcome,
            Some("success".to_string())
        );
        assert_eq!(snapshot.last_thread_route_recovery_at.is_some(), true);
        assert_eq!(
            snapshot.degraded_thread_discovery_counts,
            vec![
                GatewayV2DegradedThreadDiscoveryCounts {
                    method: "thread/list".to_string(),
                    reconnect_backoff_active: true,
                    count: 2,
                },
                GatewayV2DegradedThreadDiscoveryCounts {
                    method: "thread/loaded/list".to_string(),
                    reconnect_backoff_active: false,
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_degraded_thread_discovery_method,
            Some("thread/loaded/list".to_string())
        );
        assert_eq!(
            snapshot.last_degraded_thread_discovery_reconnect_backoff_active,
            Some(false)
        );
        assert_eq!(snapshot.last_degraded_thread_discovery_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_suppressed_notifications() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_suppressed_notification("skills/changed", "pending_refresh");
        registry.record_suppressed_notification("skills/changed", "pending_refresh");
        registry.record_suppressed_notification("warning", "client_opt_out");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.suppressed_notification_counts,
            vec![
                GatewayV2SuppressedNotificationCounts {
                    method: "skills/changed".to_string(),
                    reason: "pending_refresh".to_string(),
                    count: 2,
                },
                GatewayV2SuppressedNotificationCounts {
                    method: "warning".to_string(),
                    reason: "client_opt_out".to_string(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_suppressed_notification_method,
            Some("warning".to_string())
        );
        assert_eq!(
            snapshot.last_suppressed_notification_reason,
            Some("client_opt_out".to_string())
        );
        assert_eq!(snapshot.last_suppressed_notification_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_forwarded_notifications() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_forwarded_notification("warning");
        registry.record_forwarded_notification("item/agentMessage/delta");
        registry.record_forwarded_notification("item/agentMessage/delta");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.forwarded_notification_counts,
            vec![
                GatewayV2ForwardedNotificationCounts {
                    method: "item/agentMessage/delta".to_string(),
                    count: 2,
                },
                GatewayV2ForwardedNotificationCounts {
                    method: "warning".to_string(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_forwarded_notification_method,
            Some("item/agentMessage/delta".to_string())
        );
        assert_eq!(snapshot.last_forwarded_notification_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_notification_send_failures() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_notification_send_failure("warning", "client_send_timed_out");
        registry.record_notification_send_failure("warning", "client_send_timed_out");
        registry.record_notification_send_failure("serverRequest/resolved", "send_failed");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.notification_send_failure_counts,
            vec![
                GatewayV2NotificationSendFailureCounts {
                    method: "serverRequest/resolved".to_string(),
                    outcome: "send_failed".to_string(),
                    count: 1,
                },
                GatewayV2NotificationSendFailureCounts {
                    method: "warning".to_string(),
                    outcome: "client_send_timed_out".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.last_notification_send_failure_method,
            Some("serverRequest/resolved".to_string())
        );
        assert_eq!(
            snapshot.last_notification_send_failure_outcome,
            Some("send_failed".to_string())
        );
        assert_eq!(snapshot.last_notification_send_failure_at.is_some(), true);
    }

    #[test]
    fn snapshot_tracks_transport_and_server_request_delivery_failures() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_client_response_send_failure("model/list", "client_send_timed_out");
        registry.record_client_response_send_failure("model/list", "client_send_timed_out");
        registry.record_downstream_shutdown_failure("client_send_timed_out");
        registry.record_close_frame_send_failure(1008, "client_disconnected");
        registry.record_server_request_forward_send_failure(
            "item/tool/requestUserInput",
            "client_send_timed_out",
        );
        registry.record_server_request_forward_send_failure(
            "item/tool/requestUserInput",
            "client_send_timed_out",
        );
        registry.record_server_request_answer_delivery_failure("response");
        registry.record_server_request_answer_delivery_failure("error");
        registry
            .record_server_request_rejection_delivery_failure("item/permissions/requestApproval");

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.client_response_send_failure_counts,
            vec![GatewayV2ClientResponseSendFailureCounts {
                method: "model/list".to_string(),
                outcome: "client_send_timed_out".to_string(),
                count: 2,
            }]
        );
        assert_eq!(
            snapshot.last_client_response_send_failure_method,
            Some("model/list".to_string())
        );
        assert_eq!(
            snapshot.last_client_response_send_failure_outcome,
            Some("client_send_timed_out".to_string())
        );
        assert_eq!(
            snapshot.last_client_response_send_failure_at.is_some(),
            true
        );
        assert_eq!(
            snapshot.downstream_shutdown_failure_counts,
            vec![GatewayV2DownstreamShutdownFailureCounts {
                outcome: "client_send_timed_out".to_string(),
                count: 1,
            }]
        );
        assert_eq!(
            snapshot.last_downstream_shutdown_failure_outcome,
            Some("client_send_timed_out".to_string())
        );
        assert_eq!(snapshot.last_downstream_shutdown_failure_at.is_some(), true);
        assert_eq!(
            snapshot.close_frame_send_failure_counts,
            vec![GatewayV2CloseFrameSendFailureCounts {
                code: 1008,
                outcome: "client_disconnected".to_string(),
                count: 1,
            }]
        );
        assert_eq!(snapshot.last_close_frame_send_failure_code, Some(1008));
        assert_eq!(
            snapshot.last_close_frame_send_failure_outcome,
            Some("client_disconnected".to_string())
        );
        assert_eq!(snapshot.last_close_frame_send_failure_at.is_some(), true);
        assert_eq!(
            snapshot.server_request_forward_send_failure_counts,
            vec![GatewayV2ServerRequestForwardSendFailureCounts {
                method: "item/tool/requestUserInput".to_string(),
                outcome: "client_send_timed_out".to_string(),
                count: 2,
            }]
        );
        assert_eq!(
            snapshot.last_server_request_forward_send_failure_method,
            Some("item/tool/requestUserInput".to_string())
        );
        assert_eq!(
            snapshot.last_server_request_forward_send_failure_outcome,
            Some("client_send_timed_out".to_string())
        );
        assert_eq!(
            snapshot
                .last_server_request_forward_send_failure_at
                .is_some(),
            true
        );
        assert_eq!(
            snapshot.server_request_answer_delivery_failure_counts,
            vec![
                GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                    response_kind: "error".to_string(),
                    count: 1,
                },
                GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                    response_kind: "response".to_string(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            snapshot.last_server_request_answer_delivery_failure_response_kind,
            Some("error".to_string())
        );
        assert_eq!(
            snapshot
                .last_server_request_answer_delivery_failure_at
                .is_some(),
            true
        );
        assert_eq!(
            snapshot.server_request_rejection_delivery_failure_counts,
            vec![GatewayV2ServerRequestRejectionDeliveryFailureCounts {
                method: "item/permissions/requestApproval".to_string(),
                count: 1,
            }]
        );
        assert_eq!(
            snapshot.last_server_request_rejection_delivery_failure_method,
            Some("item/permissions/requestApproval".to_string())
        );
        assert_eq!(
            snapshot
                .last_server_request_rejection_delivery_failure_at
                .is_some(),
            true
        );
    }

    #[test]
    fn snapshot_tracks_protocol_violations() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_protocol_violation("pre_initialize", "initialize_order");
        registry.record_protocol_violation("post_initialize", "invalid_jsonrpc");
        registry.record_protocol_violation("pre_initialize", "initialize_order");
        registry.record_protocol_violation_for_worker("downstream", "invalid_jsonrpc", Some(2));
        registry.record_protocol_violation_for_worker("downstream", "invalid_jsonrpc", Some(2));
        registry.record_protocol_violation_for_worker("downstream", "invalid_frame", Some(3));

        let snapshot = registry.snapshot();

        assert_eq!(
            snapshot.protocol_violation_counts,
            vec![
                GatewayV2ProtocolViolationCounts {
                    phase: "downstream".to_string(),
                    reason: "invalid_frame".to_string(),
                    count: 1,
                },
                GatewayV2ProtocolViolationCounts {
                    phase: "downstream".to_string(),
                    reason: "invalid_jsonrpc".to_string(),
                    count: 2,
                },
                GatewayV2ProtocolViolationCounts {
                    phase: "post_initialize".to_string(),
                    reason: "invalid_jsonrpc".to_string(),
                    count: 1,
                },
                GatewayV2ProtocolViolationCounts {
                    phase: "pre_initialize".to_string(),
                    reason: "initialize_order".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot.protocol_violation_worker_counts,
            vec![
                crate::api::GatewayV2ProtocolViolationWorkerCounts {
                    worker_id: 2,
                    violation_counts: vec![GatewayV2ProtocolViolationCounts {
                        phase: "downstream".to_string(),
                        reason: "invalid_jsonrpc".to_string(),
                        count: 2,
                    }],
                },
                crate::api::GatewayV2ProtocolViolationWorkerCounts {
                    worker_id: 3,
                    violation_counts: vec![GatewayV2ProtocolViolationCounts {
                        phase: "downstream".to_string(),
                        reason: "invalid_frame".to_string(),
                        count: 1,
                    }],
                },
            ]
        );
        assert_eq!(
            snapshot.last_protocol_violation_phase,
            Some("downstream".to_string())
        );
        assert_eq!(
            snapshot.last_protocol_violation_reason,
            Some("invalid_frame".to_string())
        );
        assert_eq!(snapshot.last_protocol_violation_worker_id, Some(3));
        assert_eq!(snapshot.last_protocol_violation_at.is_some(), true);
    }
}
