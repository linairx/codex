use super::*;
use pretty_assertions::assert_eq;

#[test]
pub(crate) fn log_worker_server_request_cleanup_includes_cleaned_up_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::super::log_worker_server_request_cleanup(
            Some(1),
            "ws://worker-b.invalid",
            1,
            Some("worker-b lost"),
            &super::super::super::super::WorkerServerRequestCleanup {
                resolved_notifications: vec![
                    super::super::super::super::WorkerCleanupResolvedNotification {
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

    super::super::super::super::publish_worker_server_request_cleanup_event(
        &observability,
        Some(1),
        "ws://worker-b.invalid",
        1,
        Some("worker-b lost"),
        &super::super::super::super::WorkerServerRequestCleanup {
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
    let cleanup = super::super::super::super::WorkerServerRequestCleanup {
        resolved_notifications: vec![
            super::super::super::super::WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-visible".to_string(),
                    request_id: RequestId::String("gateway-thread-1".to_string()),
                },
                method: "item/tool/requestUserInput".to_string(),
            },
        ],
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
    let report = super::super::super::super::WorkerServerRequestCleanupReport {
        worker_websocket_url: "ws://worker-b.invalid",
        remaining_worker_count: 1,
        disconnect_message: Some("worker-b lost"),
        message: "downstream worker disconnected within shared gateway v2 session",
    };

    let logs = capture_logs(|| {
        super::super::super::super::report_worker_server_request_cleanup(
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
