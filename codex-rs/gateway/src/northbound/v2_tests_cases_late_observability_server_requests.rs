use super::*;

#[test]
pub(crate) fn log_rejected_pending_server_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        let worker_websocket_urls = [
            "ws://worker-a.invalid".to_string(),
            "ws://worker-b.invalid".to_string(),
        ];
        super::super::super::super::log_rejected_pending_server_requests(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "protocol_violation",
            Some("unexpected gateway websocket server-request response"),
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
            &worker_websocket_urls,
        );
    });

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("protocol_violation"));
    assert!(logs.contains("connection_detail"));
    assert!(logs.contains("unexpected gateway websocket server-request response"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=1"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(
        logs.contains("thread_scoped_pending_server_request_ids=[String(\"gateway-thread-1\")]")
    );
    assert!(logs.contains(
        "connection_scoped_pending_server_request_ids=[String(\"gateway-connection-1\")]"
    ));
    assert!(logs.contains("thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("worker_ids=[0, 1]"));
    assert!(
        logs.contains(
            "worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        )
    );
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
}

#[test]
pub(crate) fn log_duplicate_downstream_server_request_includes_scope_worker_and_pending_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_duplicate_downstream_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("gateway-duplicate".to_string()),
            "item/tool/requestUserInput",
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
        );
    });

    assert!(logs.contains(
            "closing gateway v2 connection because a downstream session reused a pending server-request id"
        ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("request_id=String(\"gateway-duplicate\")"));
    assert!(logs.contains("item/tool/requestUserInput"));
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
}

#[test]
pub(crate) fn log_dropped_duplicate_resolved_server_request_includes_scope_worker_and_routes() {
    let logs = capture_logs(|| {
        log_dropped_duplicate_resolved_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("downstream-duplicate".to_string()),
            &HashMap::from([(
                super::super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-remaining".to_string()),
                },
                super::super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-remaining".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(2)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "dropping duplicate downstream serverRequest/resolved replay after request-id translation"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("downstream_request_id=String(\"downstream-duplicate\")"));
    assert!(logs.contains("remaining_resolved_route_count=1"));
    assert!(
        logs.contains("remaining_resolved_gateway_request_ids=[String(\"gateway-remaining\")]")
    );
    assert!(
        logs.contains(
            "remaining_resolved_downstream_request_ids=[String(\"downstream-remaining\")]"
        )
    );
    assert!(
        logs.contains("remaining_resolved_server_request_methods=[\"item/tool/requestUserInput\"]")
    );
    assert!(logs.contains("remaining_resolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("remaining_resolved_worker_ids=[2]"));
    assert!(logs.contains("remaining_resolved_worker_websocket_urls=[\"ws://worker-c.invalid\"]"));
}
