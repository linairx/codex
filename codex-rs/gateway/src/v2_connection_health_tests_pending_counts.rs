use super::support::*;
use pretty_assertions::assert_eq;

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
