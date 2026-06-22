use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_filesystem_changed_notifications() {
    let params = serde_json::json!({
        "watchId": "watch-1",
        "changedPaths": ["/tmp/codex-gateway-fs-change.txt"],
    });
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_for_connection_notification("fs/changed", params.clone()).await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "fs/changed",
        params,
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_fuzzy_file_search_notifications() {
    let search_result = FuzzyFileSearchResult {
        root: "/tmp/project".to_string(),
        path: "docs/gateway.md".to_string(),
        match_type: FuzzyFileSearchMatchType::File,
        file_name: "gateway.md".to_string(),
        score: 42,
        indices: Some(vec![5, 6, 7, 8]),
    };
    let cases = vec![
        ServerNotification::FuzzyFileSearchSessionUpdated(
            FuzzyFileSearchSessionUpdatedNotification {
                session_id: "search-session-1".to_string(),
                query: "gate".to_string(),
                files: vec![search_result],
            },
        ),
        ServerNotification::FuzzyFileSearchSessionCompleted(
            FuzzyFileSearchSessionCompletedNotification {
                session_id: "search-session-1".to_string(),
            },
        ),
    ];

    for notification in cases {
        let initialize_response = test_initialize_response().await;
        let expected =
            tagged_type_to_notification(&notification).expect("notification should serialize");
        let websocket_url = start_mock_remote_server_for_notification(notification).await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Notification(actual) = message else {
            panic!("expected fuzzy file search notification, got {message:?}");
        };
        assert_eq!(actual, expected);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_user_visible_notifications() {
    let cases = vec![
        (
            "account/updated",
            serde_json::json!({
                "authMode": null,
                "planType": null,
            }),
        ),
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "primary",
                    "limitName": "Primary",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "warning",
            serde_json::json!({
                "threadId": null,
                "message": "Gateway warning test message",
            }),
        ),
        (
            "configWarning",
            serde_json::json!({
                "summary": "Gateway config warning summary",
                "details": null,
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "Deprecated gateway behavior",
                "details": "Use the new gateway flow instead.",
            }),
        ),
        (
            "mcpServer/startupStatus/updated",
            serde_json::json!({
                "name": "gateway-mcp",
                "status": "failed",
                "error": "handshake failed",
            }),
        ),
        (
            "account/login/completed",
            serde_json::json!({
                "loginId": "login-1",
                "success": true,
                "error": null,
            }),
        ),
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, params) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url =
            start_mock_remote_server_for_connection_notification(method, params.clone()).await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(read_websocket_message(&mut websocket).await, method, params);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_suppresses_opted_out_notifications() {
    let params = serde_json::json!({
        "threadId": null,
        "message": "Gateway opt-out test message",
    });
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_for_connection_notification("warning", params).await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Some(vec!["warning".to_string()]),
            }),
        )
        .await;

        let notification = timeout(Duration::from_millis(200), websocket.next()).await;
        assert!(notification.is_err());
    })
    .await;
    assert_v2_suppressed_notification_metric(&metrics, "warning", "opted_out");
    assert_no_v2_forwarded_notification_metric(&metrics);
    assert!(logs.contains("suppressing downstream notification opted out by northbound v2 client"));
    assert!(logs.contains("tenant-a"), "{logs}");
    assert!(logs.contains("project-a"), "{logs}");
    assert!(logs.contains("worker_websocket_url=\"ws://"), "{logs}");
    assert!(logs.contains("method=\"warning\""), "{logs}");
    assert!(logs.contains("Gateway opt-out test message"), "{logs}");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_suppresses_opted_out_multi_worker_notifications() {
    let opted_out_params = serde_json::json!({
        "threadId": null,
        "message": "Gateway multi-worker opt-out test message",
    });
    let forwarded_params = serde_json::json!({
        "summary": "Gateway multi-worker config warning",
        "details": null,
    });
    let worker_a =
        start_mock_remote_server_for_connection_notification("warning", opted_out_params).await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "configWarning",
        forwarded_params.clone(),
    )
    .await;
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_b,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
            ],
            test_initialize_response().await,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize_with_capabilities(
        &mut websocket,
        Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec!["warning".to_string()]),
        }),
    )
    .await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "configWarning",
        forwarded_params,
    );
    let notification = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(notification.is_err());
    assert_v2_suppressed_notification_metric(&metrics, "warning", "opted_out");

    server_task.abort();
    let _ = server_task.await;
}
