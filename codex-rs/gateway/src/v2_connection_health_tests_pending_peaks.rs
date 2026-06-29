use super::support::*;
use pretty_assertions::assert_eq;

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
