use super::*;
use crate::api::GatewayV2AccountCapacityWorkerEventCounts;
use crate::api::GatewayV2ClientRequestRejectionCounts;
use crate::api::GatewayV2ClientResponseSendFailureCounts;
use crate::api::GatewayV2CloseFrameSendFailureCounts;
use crate::api::GatewayV2ConnectionOutcomeCounts;
use crate::api::GatewayV2DegradedThreadDiscoveryCounts;
use crate::api::GatewayV2DownstreamBackpressureCounts;
use crate::api::GatewayV2DownstreamShutdownFailureCounts;
use crate::api::GatewayV2FailClosedRequestCounts;
use crate::api::GatewayV2ForwardedNotificationCounts;
use crate::api::GatewayV2NotificationSendFailureCounts;
use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts;
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

impl GatewayV2ConnectionHealthRegistry {
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
        let project_worker_route_selection_worker_counts = state
            .project_worker_route_selection_worker_counts
            .iter()
            .map(|(worker_id, project_worker_route_selection_count)| {
                GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                    worker_id: *worker_id,
                    project_worker_route_selection_count: *project_worker_route_selection_count,
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
            project_worker_route_selection_count: state.project_worker_route_selection_count,
            project_worker_route_selection_worker_counts,
            last_project_worker_route_selected_worker_id: state
                .last_project_worker_route_selected_worker_id,
            last_project_worker_route_selected_tenant_id: state
                .last_project_worker_route_selected_tenant_id
                .clone(),
            last_project_worker_route_selected_project_id: state
                .last_project_worker_route_selected_project_id
                .clone(),
            last_project_worker_route_selected_thread_id: state
                .last_project_worker_route_selected_thread_id
                .clone(),
            last_project_worker_route_selected_account_id: state
                .last_project_worker_route_selected_account_id
                .clone(),
            last_project_worker_route_selected_at: state.last_project_worker_route_selected_at,
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
