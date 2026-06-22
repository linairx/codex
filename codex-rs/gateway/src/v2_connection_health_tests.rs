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
use crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts;
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
            pending_client_request_method_counts: vec![GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 7,
            }],
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
            server_request_backlog_method_counts: vec![GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 5,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_count: 9,
            }],
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
    assert_eq!(active_snapshot.project_worker_route_selection_count, 0);
    assert_eq!(
        active_snapshot.project_worker_route_selection_worker_counts,
        vec![]
    );
    assert_eq!(
        active_snapshot.last_project_worker_route_selected_worker_id,
        None
    );
    assert_eq!(
        active_snapshot.last_project_worker_route_selected_tenant_id,
        None
    );
    assert_eq!(
        active_snapshot.last_project_worker_route_selected_project_id,
        None
    );
    assert_eq!(
        active_snapshot.last_project_worker_route_selected_thread_id,
        None
    );
    assert_eq!(
        active_snapshot.last_project_worker_route_selected_account_id,
        None
    );
    assert_eq!(active_snapshot.last_project_worker_route_selected_at, None);
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
            pending_client_request_method_counts: vec![GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 7,
            }],
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
fn completion_backfills_pending_client_and_backlog_timestamps_when_counts_arrive_at_teardown() {
    let registry = GatewayV2ConnectionHealthRegistry::default();
    let connection_id = registry.mark_connection_started();

    registry.mark_connection_completed(
        connection_id,
        "client_send_timed_out",
        None,
        Duration::from_millis(1),
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 1,
            pending_client_request_worker_counts: vec![GatewayV2PendingClientRequestWorkerCounts {
                worker_id: Some(7),
                pending_client_request_count: 1,
            }],
            pending_client_request_method_counts: vec![GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 1,
            }],
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: vec![GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(7),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            }],
            server_request_backlog_method_counts: vec![GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            }],
        },
    );

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.last_connection_pending_client_request_count, 1);
    assert_eq!(snapshot.last_connection_server_request_backlog_count, 2);
    assert_eq!(
        snapshot.last_connection_pending_client_request_started_at,
        snapshot.last_connection_server_request_backlog_started_at
    );
    assert_eq!(
        snapshot
            .last_connection_pending_client_request_started_at
            .is_some(),
        true
    );
    assert_eq!(
        snapshot
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        snapshot.last_connection_pending_client_request_worker_counts,
        vec![GatewayV2PendingClientRequestWorkerCounts {
            worker_id: Some(7),
            pending_client_request_count: 1,
        }]
    );
    assert_eq!(
        snapshot.last_connection_pending_client_request_method_counts,
        vec![GatewayV2PendingClientRequestMethodCounts {
            method: "command/exec".to_string(),
            pending_client_request_count: 1,
        }]
    );
    assert_eq!(
        snapshot.last_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(7),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_count: 2,
        }]
    );
    assert_eq!(
        snapshot.last_connection_server_request_backlog_method_counts,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "item/tool/requestUserInput".to_string(),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_count: 2,
        }]
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
fn snapshot_tracks_project_worker_route_selections() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        Some("acct-a"),
    );
    registry.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-b",
        Some("acct-a"),
    );
    registry.record_project_worker_route_selected(9, "tenant-b", "project-b", "thread-c", None);

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.project_worker_route_selection_count, 3);
    assert_eq!(
        snapshot.project_worker_route_selection_worker_counts,
        vec![
            GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 2,
            },
            GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 9,
                project_worker_route_selection_count: 1,
            },
        ]
    );
    assert_eq!(
        snapshot.last_project_worker_route_selected_worker_id,
        Some(9)
    );
    assert_eq!(
        snapshot
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("tenant-b")
    );
    assert_eq!(
        snapshot
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-b")
    );
    assert_eq!(
        snapshot
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-c")
    );
    assert_eq!(
        snapshot
            .last_project_worker_route_selected_account_id
            .as_deref(),
        None
    );
    assert!(snapshot.last_project_worker_route_selected_at.is_some());
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
    registry.record_server_request_rejection("item/permissions/requestApproval", "hidden_thread");

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
    registry.record_server_request_rejection_delivery_failure("item/permissions/requestApproval");

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
