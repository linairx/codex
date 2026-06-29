use super::support::*;
use pretty_assertions::assert_eq;

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
