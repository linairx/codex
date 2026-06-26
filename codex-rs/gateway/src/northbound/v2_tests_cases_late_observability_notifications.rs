use super::*;

#[test]
pub(crate) fn log_suppressed_skills_changed_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::super::log_suppressed_skills_changed_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &JSONRPCNotification {
                method: "skills/changed".to_string(),
                params: Some(serde_json::json!({"source":"worker-b"})),
            },
        );
    });

    assert!(logs.contains(
            "suppressing duplicate multi-worker skills/changed notification until the client refreshes skills/list"
        ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("method=\"skills/changed\""));
    assert!(logs.contains("source"));
    assert!(logs.contains("worker-b"));
}

#[test]
pub(crate) fn log_suppressed_opted_out_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::super::log_suppressed_opted_out_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &JSONRPCNotification {
                method: "warning".to_string(),
                params: Some(serde_json::json!({
                    "threadId": null,
                    "message": "hidden by initialize capability",
                })),
            },
        );
    });

    assert!(logs.contains("suppressing downstream notification opted out by northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("method=\"warning\""));
    assert!(logs.contains("hidden by initialize capability"));
}

#[test]
pub(crate) fn log_downstream_connect_protocol_violation_includes_scope_reason_and_message() {
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_connect_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "invalid_binary",
            "remote app server at `ws://worker-a.invalid` sent non-text initialize frame",
        );
    });

    assert!(
        logs.contains("downstream app-server sent a malformed v2 protocol frame during initialize")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("reason=\"invalid_binary\""));
    assert!(logs.contains("sent non-text initialize frame"));
}

#[test]
pub(crate) fn log_downstream_reconnect_protocol_violation_includes_scope_worker_reason_and_message()
{
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_reconnect_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            1,
            "ws://worker-b.invalid",
            "invalid_jsonrpc",
            "remote app server at `ws://worker-b.invalid` sent invalid initialize response",
        );
    });

    assert!(logs.contains(
        "downstream app-server sent a malformed v2 protocol frame during worker reconnect"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("reason=\"invalid_jsonrpc\""));
    assert!(logs.contains("sent invalid initialize response"));
}

#[test]
pub(crate) fn log_downstream_protocol_violation_includes_scope_worker_reason_and_routes() {
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "invalid_jsonrpc",
            "remote app server at `ws://worker-c.invalid` sent invalid JSON-RPC: expected value",
            2,
            &GatewayV2EventState {
                pending_server_requests: HashMap::from([(
                    RequestId::String("gateway-pending-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(2),
                        worker_websocket_url: test_worker_websocket_url(Some(2)),
                        downstream_request_id: RequestId::String(
                            "downstream-pending-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                )]),
                resolved_server_requests: HashMap::from([(
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
                skills_changed_pending_refresh: false,
                forwarded_connection_notifications: HashMap::new(),
            },
        );
    });

    assert!(logs.contains("downstream app-server sent a malformed v2 protocol frame"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("reason=\"invalid_jsonrpc\""));
    assert!(logs.contains("sent invalid JSON-RPC"));
    assert!(logs.contains("active_worker_count=2"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"gateway-pending-1\")]"));
    assert!(
        logs.contains("pending_downstream_server_request_ids=[String(\"downstream-pending-1\")]")
    );
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[2]"));
    assert!(logs.contains("pending_worker_websocket_urls=[\"ws://worker-c.invalid\"]"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
}

#[test]
pub(crate) fn log_suppressed_duplicate_connection_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::super::log_suppressed_duplicate_connection_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            Some(0),
            "ws://worker-a.invalid",
            &JSONRPCNotification {
                method: "account/updated".to_string(),
                params: Some(serde_json::json!({"requiresOpenaiAuth":true})),
            },
        );
    });

    assert!(logs.contains("suppressing exact-duplicate multi-worker connection notification"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("original_worker_id=Some(0)"));
    assert!(logs.contains("original_worker_websocket_url=\"ws://worker-a.invalid\""));
    assert!(logs.contains("method=\"account/updated\""));
    assert!(logs.contains("requiresOpenaiAuth"));
    assert!(logs.contains("true"));
}

#[test]
pub(crate) fn log_suppressed_hidden_thread_notification_includes_scope_worker_thread_and_params() {
    let logs = capture_logs(|| {
        super::super::super::super::log_suppressed_hidden_thread_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(3),
            "ws://worker-d.invalid",
            &JSONRPCNotification {
                method: "item/agentMessage/delta".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-hidden",
                    "turnId": "turn-hidden",
                    "itemId": "item-hidden",
                    "delta": "hidden text",
                })),
            },
        );
    });

    assert!(logs.contains(
        "suppressing downstream notification for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
    assert!(logs.contains("method=\"item/agentMessage/delta\""));
    assert!(logs.contains("thread-hidden"));
    assert!(logs.contains("hidden text"));
}
