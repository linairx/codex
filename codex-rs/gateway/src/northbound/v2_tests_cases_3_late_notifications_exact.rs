use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_connection_notifications() {
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

    for (method, notification_params) in cases {
        let worker_a = start_mock_remote_server_for_connection_notification(
            method,
            notification_params.clone(),
        )
        .await;
        let worker_b = start_mock_remote_server_for_connection_notification(
            method,
            notification_params.clone(),
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

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            notification_params,
        );

        let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(duplicate.is_err(), true);
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_mcp_startup_notifications() {
    let notification_params = serde_json::json!({
        "name": "gateway-mcp",
        "status": "ready",
        "error": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "mcpServer/startupStatus/updated",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "mcpServer/startupStatus/updated",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
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

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/startupStatus/updated",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(duplicate.is_err());
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/startupStatus/updated",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_login_completed_notifications()
{
    let notification_params = serde_json::json!({
        "loginId": "login-shared",
        "success": true,
        "error": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "account/login/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "account/login/completed",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
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

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/login/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "account/login/completed", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_mcp_oauth_login_completed_notifications()
 {
    let notification_params = serde_json::json!({
        "name": "shared-mcp",
        "success": true,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "mcpServer/oauthLogin/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "mcpServer/oauthLogin/completed",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
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

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/oauthLogin/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/oauthLogin/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_warning_notifications() {
    let notification_params = serde_json::json!({
        "message": "shared worker warning",
        "threadId": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "warning",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("warning", notification_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b_url = worker_b.clone();
    let metrics = in_memory_metrics();
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

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            "warning",
            notification_params,
        );

        let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(duplicate.is_err(), true);
    })
    .await;
    assert_v2_suppressed_notification_metric(&metrics, "warning", "duplicate");
    assert!(logs.contains("suppressing exact-duplicate multi-worker connection notification"));
    assert!(logs.contains("tenant-a"), "{logs}");
    assert!(logs.contains("project-a"), "{logs}");
    assert!(logs.contains("worker_id=Some(1)"), "{logs}");
    assert!(logs.contains("original_worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains(&format!("worker_websocket_url=\"{worker_b_url}\"")),
        "{logs}"
    );
    assert!(
        logs.contains(&format!("original_worker_websocket_url=\"{worker_a_url}\"")),
        "{logs}"
    );
    assert!(logs.contains("method=\"warning\""), "{logs}");
    assert!(logs.contains("shared worker warning"), "{logs}");

    server_task.abort();
    let _ = server_task.await;
}
