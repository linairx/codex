use super::*;
use crate::northbound::v2_wire::observe_v2_connection;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;

#[test]
fn observe_v2_connection_emits_terminal_detail_in_connection_log() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();

    let logs = capture_logs(|| {
        observe_v2_connection(
            &observability,
            connection_id,
            &context,
            "protocol_violation",
            Some("unexpected gateway websocket server-request response"),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
            Duration::from_millis(13),
        );
    });

    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("protocol_violation"));
    assert!(logs.contains("unexpected gateway websocket server-request response"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("13"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=2"));
}
