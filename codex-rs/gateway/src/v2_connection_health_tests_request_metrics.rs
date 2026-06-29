use super::support::*;
use pretty_assertions::assert_eq;

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
