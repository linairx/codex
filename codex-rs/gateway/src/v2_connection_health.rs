use crate::api::GatewayV2ConnectionHealth;
use std::sync::RwLock;
use std::time::Duration;
#[path = "v2_connection_health_state.rs"]
mod v2_connection_health_state;

pub use self::v2_connection_health_state::GatewayV2ConnectionCompletionCounts;
pub use self::v2_connection_health_state::GatewayV2ConnectionPendingCounts;

use self::v2_connection_health_state::ActiveGatewayV2ConnectionHealth;
use self::v2_connection_health_state::GatewayV2ConnectionHealthState;
use self::v2_connection_health_state::read_guard;
use self::v2_connection_health_state::unix_timestamp_now;
use self::v2_connection_health_state::write_guard;

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

    pub fn record_project_worker_route_selected(
        &self,
        worker_id: usize,
        tenant_id: &str,
        project_id: &str,
        thread_id: &str,
        account_id: Option<&str>,
    ) {
        let mut state = write_guard(&self.state);
        state.project_worker_route_selection_count =
            state.project_worker_route_selection_count.saturating_add(1);
        *state
            .project_worker_route_selection_worker_counts
            .entry(worker_id)
            .or_default() += 1;
        state.last_project_worker_route_selected_worker_id = Some(worker_id);
        state.last_project_worker_route_selected_tenant_id = Some(tenant_id.to_string());
        state.last_project_worker_route_selected_project_id = Some(project_id.to_string());
        state.last_project_worker_route_selected_thread_id = Some(thread_id.to_string());
        state.last_project_worker_route_selected_account_id = account_id.map(ToString::to_string);
        state.last_project_worker_route_selected_at = Some(unix_timestamp_now());
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
    ) -> GatewayV2ConnectionCompletionCounts {
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
        GatewayV2ConnectionCompletionCounts {
            max_pending_client_request_count: state
                .last_connection_max_pending_client_request_count,
            max_server_request_backlog_count: state
                .last_connection_max_server_request_backlog_count,
        }
    }
}

#[path = "v2_connection_health_snapshot.rs"]
mod v2_connection_health_snapshot;

#[cfg(test)]
#[path = "v2_connection_health_tests.rs"]
mod tests;
