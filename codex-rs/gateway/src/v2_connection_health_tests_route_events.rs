use super::support::*;
use pretty_assertions::assert_eq;

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
