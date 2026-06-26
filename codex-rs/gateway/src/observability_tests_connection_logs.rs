use super::*;

#[test]
fn emits_info_log_for_normal_v2_connection_outcome() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_log(
            "client_disconnected",
            Duration::from_millis(7),
            &context,
            None,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 4,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(2),
                        pending_client_request_count: 4,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 4,
                    },
                ],
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                ],
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 9,
                max_server_request_backlog_count: 8,
            },
        );
    });

    assert!(logs.contains("INFO"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("client_disconnected"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("7"));
    assert!(logs.contains("pending_client_request_count=4"));
    assert!(logs.contains("max_pending_client_request_count=9"));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("method: \"command/exec\""));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(logs.contains("max_server_request_backlog_count=8"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(2)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"item/tool/requestUserInput\""));
}

#[test]
fn emits_warn_log_for_non_normal_v2_connection_outcome() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: None,
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_log(
            "downstream_backpressure",
            Duration::from_millis(11),
            &context,
            Some("downstream app-server event stream lagged"),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 5,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 6,
                max_server_request_backlog_count: 7,
            },
        );
    });

    assert!(logs.contains("WARN"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("downstream_backpressure"));
    assert!(logs.contains("downstream app-server event stream lagged"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("11"));
    assert!(logs.contains("pending_client_request_count=5"));
    assert!(logs.contains("max_pending_client_request_count=6"));
    assert!(logs.contains("pending_server_request_count=3"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=2"));
    assert!(logs.contains("server_request_backlog_count=5"));
    assert!(logs.contains("max_server_request_backlog_count=7"));
}

#[test]
fn emits_v2_connection_audit_log_with_server_request_counts() {
    let observability = GatewayObservability::new(None, true);
    let context = GatewayRequestContext {
        tenant_id: "tenant-audit".to_string(),
        project_id: Some("project-audit".to_string()),
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_audit_log(
            "client_send_timed_out",
            Duration::from_millis(17),
            &context,
            Some("gateway websocket send timed out"),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(7),
                        pending_client_request_count: 6,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 6,
                    },
                ],
                pending_server_request_count: 4,
                answered_but_unresolved_server_request_count: 3,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(7),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 7,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "serverRequest/elicitation".to_string(),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 7,
                    },
                ],
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 10,
                max_server_request_backlog_count: 11,
            },
        );
    });

    assert!(logs.contains("codex_gateway.audit"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("client_send_timed_out"));
    assert!(logs.contains("tenant-audit"));
    assert!(logs.contains("project-audit"));
    assert!(logs.contains("17"));
    assert!(logs.contains("gateway websocket send timed out"));
    assert!(logs.contains("pending_client_request_count=6"));
    assert!(logs.contains("max_pending_client_request_count=10"));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("method: \"command/exec\""));
    assert!(logs.contains("pending_server_request_count=4"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=3"));
    assert!(logs.contains("server_request_backlog_count=7"));
    assert!(logs.contains("max_server_request_backlog_count=11"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(7)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"serverRequest/elicitation\""));
}
