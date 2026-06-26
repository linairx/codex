use super::*;

#[test]
pub(crate) fn log_unexpected_client_server_request_response_includes_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_unexpected_client_server_request_response(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "response",
            &RequestId::String("gateway-unexpected".to_string()),
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

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("response"));
    assert!(logs.contains("unexpected_request_id=String(\"gateway-unexpected\")"));
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
pub(crate) fn log_unexpected_client_server_request_error_includes_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_unexpected_client_server_request_response(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "error",
            &RequestId::String("gateway-unexpected".to_string()),
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

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("error"));
    assert!(logs.contains("unexpected_request_id=String(\"gateway-unexpected\")"));
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
pub(crate) fn log_rejected_saturated_server_request_includes_scope_worker_and_pending_request_ids()
{
    let logs = capture_logs(|| {
        super::super::super::super::log_rejected_saturated_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("gateway-incoming".to_string()),
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
            2,
        );
    });

    assert!(logs.contains(
        "rejecting downstream server request because the gateway websocket connection is saturated"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("limit=2"));
    assert!(logs.contains("request_id=String(\"gateway-incoming\")"));
    assert!(logs.contains("item/tool/requestUserInput"));
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
pub(crate) fn log_rejected_saturated_client_request_includes_scope_method_and_limit() {
    let logs = capture_logs(|| {
        super::super::super::super::log_rejected_saturated_client_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("command-exec-2".to_string()),
            "command/exec",
            1,
            1,
        );
    });

    assert!(logs.contains(
        "rejecting client request because the gateway websocket connection is saturated"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"command-exec-2\")"));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("limit=1"));
}
