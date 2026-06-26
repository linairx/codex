use super::*;

#[test]
pub(crate) fn log_notification_send_failure_includes_scope_worker_method_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::super::log_notification_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "warning",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver downstream notification to northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("method=\"warning\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_downstream_server_request_forward_failure_includes_scope_worker_and_method() {
    let logs = capture_logs(|| {
        super::super::super::super::log_downstream_server_request_forward_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "item/commandExecution/requestApproval",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver downstream server request to northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("method=\"item/commandExecution/requestApproval\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_client_response_send_failure_includes_scope_request_method_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::super::log_client_response_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("client-request-1".to_string()),
            "model/list",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(
        logs.contains("failed to deliver gateway v2 client request response to northbound client")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"client-request-1\")"));
    assert!(logs.contains("method=\"model/list\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_close_frame_send_failure_includes_scope_code_reason_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::super::log_close_frame_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            close_code::POLICY,
            "gateway initialize timed out",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver gateway v2 close frame to northbound client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("code=1008"));
    assert!(logs.contains("reason=\"gateway initialize timed out\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}
