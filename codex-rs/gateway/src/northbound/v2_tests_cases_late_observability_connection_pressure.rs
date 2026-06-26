use super::*;

#[test]
pub(crate) fn log_duplicate_pending_client_request_includes_scope_method_and_active_routes() {
    let logs = capture_logs(|| {
        super::super::super::super::log_duplicate_pending_client_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("command-exec-1".to_string()),
            "command/exec",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because the northbound client reused a pending request id"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"command-exec-1\")"));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_rejected_hidden_downstream_server_request_includes_scope_worker_and_thread_id() {
    let logs = capture_logs(|| {
        super::super::super::super::log_rejected_hidden_downstream_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-hidden".to_string(),
                project_id: Some("project-hidden".to_string()),
            },
            Some(3),
            "ws://worker-d.invalid",
            &RequestId::String("gateway-hidden".to_string()),
            "item/commandExecution/requestApproval",
            Some("thread-hidden"),
        );
    });

    assert!(logs.contains(
        "rejecting downstream server request for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-hidden"));
    assert!(logs.contains("project-hidden"));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
    assert!(logs.contains("request_id=String(\"gateway-hidden\")"));
    assert!(logs.contains("item/commandExecution/requestApproval"));
    assert!(logs.contains("thread-hidden"));
}

#[test]
pub(crate) fn log_downstream_backpressure_close_includes_scope_worker_and_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_backpressure_close(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            3,
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(2)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because the downstream app-server event stream lagged"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("skipped_event_count=3"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[2]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-c.invalid\"]")
    );
}

#[test]
pub(crate) fn log_client_send_timeout_includes_scope_detail_and_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_client_send_timeout(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "gateway websocket send timed out",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because sending to the northbound client timed out"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_downstream_shutdown_failure_includes_scope_outcome_and_pending_counts() {
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_shutdown_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_send_timed_out",
            Some("gateway websocket send timed out"),
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
            &crate::v2_connection_health::GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 1,
                pending_client_request_worker_counts: vec![
                    crate::api::GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(1),
                        pending_client_request_count: 1,
                    },
                ],
                pending_client_request_method_counts: vec![
                    crate::api::GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 1,
                    },
                ],
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 3,
                server_request_backlog_worker_counts: vec![
                    crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 5,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 5,
                    },
                ],
            },
            &std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "downstream shutdown channel closed",
            ),
        );
    });

    assert!(
        logs.contains(
            "gateway v2 websocket downstream shutdown also failed after connection error"
        )
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_outcome=\"client_send_timed_out\""));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_count: 1"));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=3"));
    assert!(logs.contains("server_request_backlog_count=5"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(2)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"item/tool/requestUserInput\""));
    assert!(logs.contains("downstream shutdown channel closed"));
}
