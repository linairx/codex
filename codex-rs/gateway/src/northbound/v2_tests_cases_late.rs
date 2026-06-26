use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_late_notifications.rs"]
mod v2_tests_cases_late_notifications;

#[path = "v2_tests_cases_late_notification_dedupe.rs"]
mod v2_tests_cases_late_notification_dedupe;

#[path = "v2_tests_cases_late_route_diagnostics.rs"]
mod v2_tests_cases_late_route_diagnostics;

#[test]
pub(crate) fn log_worker_server_request_cleanup_includes_cleaned_up_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_worker_server_request_cleanup(
            Some(1),
            "ws://worker-b.invalid",
            1,
            Some("worker-b lost"),
            &super::super::super::WorkerServerRequestCleanup {
                resolved_notifications: vec![
                    super::super::super::WorkerCleanupResolvedNotification {
                        notification: ServerRequestResolvedNotification {
                            thread_id: "thread-visible".to_string(),
                            request_id: RequestId::String("gateway-thread-1".to_string()),
                        },
                        method: "item/tool/requestUserInput".to_string(),
                    },
                ],
                resolved_thread_scoped_requests: 1,
                resolved_thread_scoped_request_ids: vec![RequestId::String(
                    "gateway-thread-1".to_string(),
                )],
                resolved_thread_scoped_downstream_request_ids: vec![RequestId::String(
                    "downstream-thread-1".to_string(),
                )],
                resolved_thread_scoped_methods: vec!["item/tool/requestUserInput".to_string()],
                resolved_thread_scoped_thread_ids: vec!["thread-visible".to_string()],
                stranded_connection_scoped_requests: 1,
                stranded_connection_scoped_request_ids: vec![RequestId::String(
                    "gateway-connection-1".to_string(),
                )],
                stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
                    "downstream-connection-1".to_string(),
                )],
                stranded_connection_scoped_methods: vec![
                    "account/chatgptAuthTokens/refresh".to_string(),
                ],
            },
            "downstream worker disconnected within shared gateway v2 session",
        );
    });

    assert!(logs.contains("downstream worker disconnected within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("remaining_worker_count=1"));
    assert!(logs.contains("disconnect_message"));
    assert!(logs.contains("worker-b lost"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=1"));
    assert!(
        logs.contains("resolved_thread_scoped_server_request_ids=[String(\"gateway-thread-1\")]")
    );
    assert!(logs.contains(
        "resolved_thread_scoped_downstream_server_request_ids=[String(\"downstream-thread-1\")]"
    ));
    assert!(logs.contains(
        "resolved_thread_scoped_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("resolved_thread_scoped_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=1"));
    assert!(logs.contains(
        "stranded_connection_scoped_server_request_ids=[String(\"gateway-connection-1\")]"
    ));
    assert!(logs.contains(
            "stranded_connection_scoped_downstream_server_request_ids=[String(\"downstream-connection-1\")]"
        ));
    assert!(logs.contains(
        "stranded_connection_scoped_server_request_methods=[\"account/chatgptAuthTokens/refresh\"]"
    ));
}

#[test]
pub(crate) fn worker_server_request_cleanup_publishes_operator_event() {
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let observability = GatewayObservability::default().with_operator_events(operator_events_tx);

    super::super::super::publish_worker_server_request_cleanup_event(
        &observability,
        Some(1),
        "ws://worker-b.invalid",
        1,
        Some("worker-b lost"),
        &super::super::super::WorkerServerRequestCleanup {
            resolved_notifications: Vec::new(),
            resolved_thread_scoped_requests: 2,
            resolved_thread_scoped_request_ids: vec![
                RequestId::String("gateway-thread-2".to_string()),
                RequestId::String("gateway-thread-1".to_string()),
            ],
            resolved_thread_scoped_downstream_request_ids: vec![
                RequestId::String("downstream-thread-2".to_string()),
                RequestId::String("downstream-thread-1".to_string()),
            ],
            resolved_thread_scoped_methods: vec![
                "item/tool/requestUserInput".to_string(),
                "item/tool/requestUserInput".to_string(),
            ],
            resolved_thread_scoped_thread_ids: vec![
                "thread-visible".to_string(),
                "thread-visible".to_string(),
            ],
            stranded_connection_scoped_requests: 1,
            stranded_connection_scoped_request_ids: vec![RequestId::String(
                "gateway-connection-1".to_string(),
            )],
            stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
                "downstream-connection-1".to_string(),
            )],
            stranded_connection_scoped_methods: vec![
                "account/chatgptAuthTokens/refresh".to_string(),
            ],
        },
    );

    let event = operator_events_rx
        .try_recv()
        .expect("cleanup operator event should publish");
    assert_eq!(event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        serde_json::json!({
            "workerId": 1,
            "workerWebsocketUrl": "ws://worker-b.invalid",
            "remainingWorkerCount": 1,
            "disconnectMessage": "worker-b lost",
            "resolvedThreadScopedServerRequestCount": 2,
            "resolvedThreadScopedServerRequestIds": [
                "gateway-thread-1",
                "gateway-thread-2"
            ],
            "resolvedThreadScopedDownstreamServerRequestIds": [
                "downstream-thread-1",
                "downstream-thread-2"
            ],
            "resolvedThreadScopedServerRequestMethods": ["item/tool/requestUserInput"],
            "resolvedThreadScopedThreadIds": ["thread-visible"],
            "strandedConnectionScopedServerRequestCount": 1,
            "strandedConnectionScopedServerRequestIds": ["gateway-connection-1"],
            "strandedConnectionScopedDownstreamServerRequestIds": [
                "downstream-connection-1"
            ],
            "strandedConnectionScopedServerRequestMethods": [
                "account/chatgptAuthTokens/refresh"
            ],
        })
    );
}

#[test]
pub(crate) fn worker_server_request_cleanup_report_logs_and_publishes_operator_event() {
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let observability = GatewayObservability::default().with_operator_events(operator_events_tx);
    let cleanup = super::super::super::WorkerServerRequestCleanup {
        resolved_notifications: vec![super::super::super::WorkerCleanupResolvedNotification {
            notification: ServerRequestResolvedNotification {
                thread_id: "thread-visible".to_string(),
                request_id: RequestId::String("gateway-thread-1".to_string()),
            },
            method: "item/tool/requestUserInput".to_string(),
        }],
        resolved_thread_scoped_requests: 1,
        resolved_thread_scoped_request_ids: vec![RequestId::String("gateway-thread-1".to_string())],
        resolved_thread_scoped_downstream_request_ids: vec![RequestId::String(
            "downstream-thread-1".to_string(),
        )],
        resolved_thread_scoped_methods: vec!["item/tool/requestUserInput".to_string()],
        resolved_thread_scoped_thread_ids: vec!["thread-visible".to_string()],
        stranded_connection_scoped_requests: 1,
        stranded_connection_scoped_request_ids: vec![RequestId::String(
            "gateway-connection-1".to_string(),
        )],
        stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
            "downstream-connection-1".to_string(),
        )],
        stranded_connection_scoped_methods: vec!["account/chatgptAuthTokens/refresh".to_string()],
    };
    let report = super::super::super::WorkerServerRequestCleanupReport {
        worker_websocket_url: "ws://worker-b.invalid",
        remaining_worker_count: 1,
        disconnect_message: Some("worker-b lost"),
        message: "downstream worker disconnected within shared gateway v2 session",
    };

    let logs = capture_logs(|| {
        super::super::super::report_worker_server_request_cleanup(
            &observability,
            Some(1),
            &report,
            &cleanup,
        );
    });

    assert!(logs.contains("downstream worker disconnected within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    let event = operator_events_rx
        .try_recv()
        .expect("cleanup operator event should publish");
    assert_eq!(event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(
        event.data["resolvedThreadScopedServerRequestIds"],
        serde_json::json!(["gateway-thread-1"])
    );
    assert_eq!(
        event.data["strandedConnectionScopedServerRequestIds"],
        serde_json::json!(["gateway-connection-1"])
    );
}

#[test]
pub(crate) fn log_unexpected_client_server_request_response_includes_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_unexpected_client_server_request_response(
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_unexpected_client_server_request_response(
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_rejected_saturated_server_request(
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
        super::super::super::log_rejected_saturated_client_request(
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

#[test]
pub(crate) fn log_notification_send_failure_includes_scope_worker_method_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::log_notification_send_failure(
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
        super::super::super::log_downstream_server_request_forward_failure(
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
        super::super::super::log_client_response_send_failure(
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
        super::super::super::log_close_frame_send_failure(
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

#[test]
pub(crate) fn log_duplicate_pending_client_request_includes_scope_method_and_active_routes() {
    let logs = capture_logs(|| {
        super::super::super::log_duplicate_pending_client_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("command-exec-1".to_string()),
            "command/exec",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
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
        super::super::super::log_rejected_hidden_downstream_server_request(
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
        super::super::super::log_downstream_backpressure_close(
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_client_send_timeout(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "gateway websocket send timed out",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_downstream_shutdown_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_send_timed_out",
            Some("gateway websocket send timed out"),
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
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

#[test]
pub(crate) fn pending_client_responses_settle_completed_responses_before_teardown() {
    let (tx, mut rx) = mpsc::channel(2);
    let mut pending_client_responses = super::super::super::PendingClientResponses {
        tx,
        tasks: Vec::new(),
        count: 2,
        active: HashMap::from([
            (
                RequestId::String("command-exec-complete".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
            (
                RequestId::String("command-exec-pending".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
        ]),
    };

    pending_client_responses
        .tx
        .try_send(super::super::super::PendingClientResponse {
            request_id: RequestId::String("command-exec-complete".to_string()),
            request_context: GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            method: "command/exec".to_string(),
            started_at: Instant::now(),
            result: Ok(Ok(serde_json::json!({ "exitCode": 0 }))),
        })
        .expect("completed background response should enqueue");

    pending_client_responses.settle_completed_responses(&mut rx);

    assert_eq!(pending_client_responses.count, 1);
    assert_eq!(
        pending_client_responses
            .active
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![RequestId::String("command-exec-pending".to_string())]
    );
}

#[test]
pub(crate) fn log_aborted_pending_client_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_aborted_pending_client_requests(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_disconnected",
            Some("gateway websocket receive failed: closed"),
            &HashMap::from([
                (
                    RequestId::String("command-exec-1".to_string()),
                    super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(0),
                        worker_websocket_url: "ws://worker-a.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
                (
                    RequestId::String("command-exec-2".to_string()),
                    super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(1),
                        worker_websocket_url: "ws://worker-b.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
            ]),
        );
    });

    assert!(logs.contains(
        "aborting pending gateway v2 client requests because the northbound connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("outcome=\"client_disconnected\""));
    assert!(logs.contains("detail=\"gateway websocket receive failed: closed\""));
    assert!(logs.contains("pending_client_request_count=2"));
    assert!(logs.contains(
        "pending_client_request_ids=[String(\"command-exec-1\"), String(\"command-exec-2\")]"
    ));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\", \"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[0, 1]"));
    assert!(logs.contains(
            "pending_client_request_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        ));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("worker_id: Some(0)"));
    assert!(logs.contains("worker_id: Some(1)"));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("pending_client_request_count: 2"));
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_request_metrics_and_audit_log() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_disconnected",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_disconnected\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_disconnected", 1)]);
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_client_send_timeout_outcome() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_send_timed_out",
            &HashMap::from([(
                RequestId::String("command-exec-timeout".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_send_timed_out\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_send_timed_out", 1)]);
}

#[test]
pub(crate) fn log_rejected_pending_server_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        let worker_websocket_urls = [
            "ws://worker-a.invalid".to_string(),
            "ws://worker-b.invalid".to_string(),
        ];
        super::super::super::log_rejected_pending_server_requests(
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_duplicate_downstream_server_request(
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
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-remaining".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
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

#[test]
pub(crate) fn log_suppressed_skills_changed_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::log_suppressed_skills_changed_notification(
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
        super::super::super::log_suppressed_opted_out_notification(
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
        super::super::super::log_downstream_connect_protocol_violation(
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
        super::super::super::log_downstream_reconnect_protocol_violation(
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
        super::super::super::log_downstream_protocol_violation(
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
                    super::super::super::DownstreamServerRequestKey {
                        worker_id: Some(1),
                        request_id: RequestId::String("downstream-resolved-1".to_string()),
                    },
                    super::super::super::ResolvedServerRequestRoute {
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
        super::super::super::log_suppressed_duplicate_connection_notification(
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
        super::super::super::log_suppressed_hidden_thread_notification(
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

#[test]
pub(crate) fn log_deduplicated_thread_list_entry_includes_selected_and_discarded_workers() {
    let logs = capture_logs(|| {
        log_deduplicated_thread_list_entry(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            DeduplicatedThreadListEntryLog {
                thread_id: "thread-visible",
                selected_worker_id: Some(2),
                selected_worker_websocket_url: "ws://worker-c.invalid",
                discarded_worker_id: Some(7),
                discarded_worker_websocket_url: "ws://worker-h.invalid",
                selected_updated_at: 40,
                discarded_updated_at: 32,
                selected_created_at: 12,
                discarded_created_at: 8,
            },
        );
    });

    assert!(logs.contains("deduplicating repeated thread/list entry across downstream workers"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("selected_worker_id=Some(2)"));
    assert!(logs.contains("selected_worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("discarded_worker_id=Some(7)"));
    assert!(logs.contains("discarded_worker_websocket_url=\"ws://worker-h.invalid\""));
    assert!(logs.contains("selected_updated_at=40"));
    assert!(logs.contains("discarded_updated_at=32"));
    assert!(logs.contains("selected_created_at=12"));
    assert!(logs.contains("discarded_created_at=8"));
}

#[test]
pub(crate) fn log_recovered_visible_thread_worker_route_includes_scope_and_worker() {
    let logs = capture_logs(|| {
        super::super::super::log_recovered_visible_thread_worker_route(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            Some(3),
            "ws://worker-d.invalid",
        );
    });

    assert!(
        logs.contains("recovered missing visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
}

#[test]
pub(crate) fn log_failed_visible_thread_worker_route_recovery_includes_attempted_workers() {
    let logs = capture_logs(|| {
        super::super::super::log_failed_visible_thread_worker_route_recovery(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            &[Some(1), Some(4), None],
            &[
                "ws://worker-b.invalid",
                "ws://worker-e.invalid",
                "<unknown>",
            ],
        );
    });

    assert!(
        logs.contains("failed to recover visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("attempted_worker_ids=[Some(1), Some(4), None]"));
    assert!(logs.contains(
            "attempted_worker_websocket_urls=[\"ws://worker-b.invalid\", \"ws://worker-e.invalid\", \"<unknown>\"]"
        ));
}

pub(crate) async fn start_mock_remote_server_expecting_forwarded_initialized() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let frame = websocket
            .next()
            .await
            .expect("forwarded initialized frame should exist")
            .expect("forwarded initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected forwarded initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("forwarded initialized should decode")
        else {
            panic!("expected forwarded initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_initialize_with_expected_headers(
    expected_tenant_id: &str,
    expected_project_id: Option<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_tenant_id = expected_tenant_id.to_string();
    let expected_project_id = expected_project_id.map(str::to_string);
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-tenant-id")
                        .and_then(|value| value.to_str().ok()),
                    Some(expected_tenant_id.as_str())
                );
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-project-id")
                        .and_then(|value| value.to_str().ok()),
                    expected_project_id.as_deref()
                );
                Ok(response)
            },
        )
        .await
        .expect("websocket should accept");
        expect_remote_initialize(&mut websocket).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_hidden_server_request(
    request: JSONRPCRequest,
    expected_error_message: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_request_id = request.id.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(request.clone()))
                    .expect("server request should serialize")
                    .into(),
            ))
            .await
            .expect("server request should send");

        let frame = websocket
            .next()
            .await
            .expect("server request response should exist")
            .expect("server request response should decode");
        let Message::Text(text) = frame else {
            panic!("expected server request response text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&text).expect("server request response should decode")
        else {
            panic!("expected server request error");
        };
        assert_eq!(error.id, expected_request_id);
        assert_eq!(error.error.code, super::super::super::INVALID_PARAMS_CODE);
        assert_eq!(error.error.message, expected_error_message);

        tokio::time::sleep(Duration::from_millis(250)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_thread_start_then_server_requests()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let frame = websocket
                        .next()
                        .await
                        .expect("initialized frame should exist")
                        .expect("initialized frame should decode");
                    let Message::Text(text) = frame else {
                        panic!("expected initialized text frame");
                    };
                    let JSONRPCMessage::Notification(notification) =
                        serde_json::from_str(&text).expect("initialized should decode")
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");

                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "thread/start");
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "thread": {
                                        "id": "thread-recovered",
                                        "forkedFromId": null,
                                        "preview": "",
                                        "ephemeral": true,
                                        "modelProvider": "openai",
                                        "createdAt": 1,
                                        "updatedAt": 1,
                                        "status": {
                                            "type": "idle",
                                        },
                                        "path": null,
                                        "cwd": "/tmp/recovered-worker",
                                        "cliVersion": "0.0.0-test",
                                        "source": "cli",
                                        "agentNickname": null,
                                        "agentRole": null,
                                        "gitInfo": null,
                                        "name": null,
                                        "turns": [],
                                    },
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": "/tmp/recovered-worker",
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess",
                                    },
                                    "reasoningEffort": null,
                                }),
                            }))
                            .expect("thread/start response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("thread/start response should send");

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-user-input".to_string()),
                                method: "item/tool/requestUserInput".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-recovered",
                                    "turnId": "turn-recovered",
                                    "itemId": "tool-call-recovered",
                                    "questions": [{
                                        "id": "mode",
                                        "header": "Mode",
                                        "question": "Pick execution mode",
                                        "isOther": false,
                                        "isSecret": false,
                                        "options": [],
                                    }],
                                })),
                                trace: None,
                            }))
                            .expect("user-input request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("user-input request should send");

                    let JSONRPCMessage::Response(user_input_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected user-input response");
                    };
                    assert_eq!(
                        user_input_response.id,
                        RequestId::String("downstream-user-input".to_string())
                    );
                    assert_eq!(
                        user_input_response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": ["safe"],
                                },
                            },
                        })
                    );

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-refresh".to_string()),
                                method: "account/chatgptAuthTokens/refresh".to_string(),
                                params: Some(serde_json::json!({
                                    "reason": "unauthorized",
                                    "previousAccountId": "acct-recovered",
                                })),
                                trace: None,
                            }))
                            .expect("refresh request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("refresh request should send");

                    let JSONRPCMessage::Response(refresh_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected refresh response");
                    };
                    assert_eq!(
                        refresh_response.id,
                        RequestId::String("downstream-refresh".to_string())
                    );
                    assert_eq!(
                        refresh_response.result,
                        serde_json::json!({
                            "accessToken": "access-token-recovered",
                            "chatgptAccountId": "acct-recovered",
                            "chatgptPlanType": "pro",
                        })
                    );

                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_primary_login_completed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "account/login/start");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "type": "chatgpt",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "type": "chatgpt",
                                    "loginId": "login-reconnected",
                                    "authUrl": "https://example.com/login",
                                }),
                            }))
                            .expect("login response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("login response should send");
                    send_remote_notification(
                        &mut websocket,
                        "account/login/completed",
                        serde_json::json!({
                            "loginId": "login-reconnected",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_mcp_oauth_login_completed() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "mcpServer/oauth/login");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "name": "shared-mcp",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "authorizationUrl": "https://example.com/oauth/shared-mcp",
                                }),
                            }))
                            .expect("oauth response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("oauth response should send");
                    send_remote_notification(
                        &mut websocket,
                        "mcpServer/oauthLogin/completed",
                        serde_json::json!({
                            "name": "shared-mcp",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let JSONRPCMessage::Notification(initialized) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(initialized.method, "initialized");
                    assert_eq!(initialized.params, None);

                    let replay_watch_request = read_websocket_request(&mut websocket).await;
                    assert_eq!(replay_watch_request.method, "fs/watch");
                    assert_eq!(
                        replay_watch_request.id,
                        RequestId::String("gateway-replay-fs-watch:watch-shared".to_string())
                    );
                    assert_json_params_eq(
                        replay_watch_request.params,
                        Some(serde_json::json!({
                            "watchId": "watch-shared",
                            "path": "/tmp/shared/project/.git/HEAD",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: replay_watch_request.id,
                                result: serde_json::json!({
                                    "path": "/tmp/shared/project/.git/HEAD",
                                }),
                            }))
                            .expect("replayed fs/watch response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("replayed fs/watch response should send");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_replay_failure() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let JSONRPCMessage::Notification(initialized) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(initialized.method, "initialized");
        assert_eq!(initialized.params, None);

        websocket
            .close(None)
            .await
            .expect("close frame should send before replayed fs/watch");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_item_delta_notification() -> String {
    start_mock_remote_server_for_notification(ServerNotification::AgentMessageDelta(
        codex_app_server_protocol::AgentMessageDeltaNotification {
            thread_id: "thread-visible".to_string(),
            turn_id: "turn-visible".to_string(),
            item_id: "item-visible".to_string(),
            delta: "streamed text".to_string(),
        },
    ))
    .await
}

pub(crate) async fn start_mock_remote_server_for_notification(
    notification: ServerNotification,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let notification =
            tagged_type_to_notification(notification).expect("notification should serialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Notification(notification))
                    .expect("notification should serialize")
                    .into(),
            ))
            .await
            .expect("notification should send");

        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    format!("ws://{addr}")
}
