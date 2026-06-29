use super::support::*;
use pretty_assertions::assert_eq;

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
