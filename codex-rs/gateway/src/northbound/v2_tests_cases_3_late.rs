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

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_mcp_oauth_notifications() {
    let first_params = serde_json::json!({
        "name": "shared-mcp",
        "success": true,
    });
    let second_params = serde_json::json!({
        "name": "other-mcp",
        "success": true,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("mcpServer/oauthLogin/completed", first_params.clone()),
        ("mcpServer/oauthLogin/completed", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("mcpServer/oauthLogin/completed", first_params.clone())],
        Duration::from_millis(50),
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
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/oauthLogin/completed",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "mcpServer/oauthLogin/completed"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate mcpServer/oauthLogin/completed notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/oauthLogin/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_mcp_startup_notifications() {
    let first_params = serde_json::json!({
        "name": "shared-mcp",
        "status": "ready",
        "error": null,
    });
    let second_params = serde_json::json!({
        "name": "other-mcp",
        "status": "failed",
        "error": "startup failed",
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("mcpServer/startupStatus/updated", first_params.clone()),
        ("mcpServer/startupStatus/updated", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("mcpServer/startupStatus/updated", first_params.clone())],
        Duration::from_millis(50),
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
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/startupStatus/updated",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "mcpServer/startupStatus/updated"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate mcpServer/startupStatus/updated notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/startupStatus/updated",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_account_updates() {
    let first_params = serde_json::json!({
        "authMode": null,
        "planType": null,
    });
    let second_params = serde_json::json!({
        "authMode": "apikey",
        "planType": null,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("account/updated", first_params.clone()),
        ("account/updated", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("account/updated", first_params.clone())],
        Duration::from_millis(50),
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
        "account/updated",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/updated",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "account/updated"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate account/updated notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "account/updated", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_login_completed_notifications() {
    let first_params = serde_json::json!({
        "loginId": null,
        "success": true,
        "error": null,
    });
    let second_params = serde_json::json!({
        "loginId": "other-login",
        "success": false,
        "error": "cancelled",
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("account/login/completed", first_params.clone()),
        ("account/login/completed", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("account/login/completed", first_params.clone())],
        Duration::from_millis(50),
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
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/login/completed",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "account/login/completed"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate account/login/completed notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "account/login/completed", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_discovery_state_notifications() {
    let cases = vec![
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "shared",
                    "limitName": "Shared",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "other",
                    "limitName": "Other",
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
                "data": [{
                    "id": "shared-app",
                    "name": "Shared App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
            serde_json::json!({
                "data": [{
                    "id": "other-app",
                    "name": "Other App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
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
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_windows_setup_notifications() {
    let cases = vec![
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/shared-world-writable"],
                "extraCount": 1,
                "failedScan": false,
            }),
            serde_json::json!({
                "samplePaths": ["/tmp/other-world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "mode": "elevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
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
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_warning_notifications() {
    let first_params = serde_json::json!({
        "message": "first shared worker warning",
        "threadId": null,
    });
    let second_params = serde_json::json!({
        "message": "second shared worker warning",
        "threadId": null,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("warning", first_params.clone()),
        ("warning", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("warning", first_params.clone())],
        Duration::from_millis(50),
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
        "warning",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "warning",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "warning"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate warning notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "warning", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_visible_notice_notifications() {
    let cases = vec![
        (
            "configWarning",
            serde_json::json!({
                "summary": "first shared config warning",
                "details": null,
            }),
            serde_json::json!({
                "summary": "second shared config warning",
                "details": "different config warning detail",
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "first shared deprecation notice",
                "details": null,
            }),
            serde_json::json!({
                "summary": "second shared deprecation notice",
                "details": "different deprecation detail",
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
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
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_preserves_same_worker_repeated_connection_notifications() {
    let cases = vec![
        (
            "account/updated",
            serde_json::json!({
                "authMode": null,
                "planType": null,
            }),
            serde_json::json!({
                "authMode": "apikey",
                "planType": null,
            }),
        ),
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "shared",
                    "limitName": "Shared",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "other",
                    "limitName": "Other",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "account/login/completed",
            serde_json::json!({
                "loginId": null,
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "loginId": "other-login",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [],
            }),
            serde_json::json!({
                "data": [{
                    "id": "other-app",
                    "name": "Other App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
        ),
        (
            "configWarning",
            serde_json::json!({
                "summary": "repeated worker config warning",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker config warning",
                "details": "different detail",
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "repeated worker deprecation notice",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker deprecation notice",
                "details": "different detail",
            }),
        ),
        (
            "externalAgentConfig/import/completed",
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
        ),
        (
            "mcpServer/oauthLogin/completed",
            serde_json::json!({
                "name": "shared-mcp",
                "success": true,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "mcpServer/startupStatus/updated",
            serde_json::json!({
                "name": "shared-mcp",
                "status": "ready",
                "error": null,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "status": "failed",
                "error": "startup failed",
            }),
        ),
        (
            "warning",
            serde_json::json!({
                "message": "repeated worker warning",
                "threadId": null,
            }),
            serde_json::json!({
                "message": "other worker warning",
                "threadId": null,
            }),
        ),
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/repeated-world-writable"],
                "extraCount": 1,
                "failedScan": false,
            }),
            serde_json::json!({
                "samplePaths": ["/tmp/other-world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "mode": "elevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, repeated_params, other_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, repeated_params.clone()),
            (method, other_params.clone()),
            (method, repeated_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications(Vec::new()).await;
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
            method,
            repeated_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            other_params,
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            repeated_params,
        );
        assert_no_v2_suppressed_notification_metric(&metrics);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_config_warning_notifications()
{
    let notification_params = serde_json::json!({
        "summary": "shared config warning",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "configWarning",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "configWarning",
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
        "configWarning",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "configWarning", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_deprecation_notice_notifications()
 {
    let notification_params = serde_json::json!({
        "summary": "shared deprecation notice",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
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
        "deprecationNotice",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "deprecationNotice", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_external_agent_import_completed_notifications()
 {
    let notification_params = serde_json::json!({
        "importId": "import-1",
        "itemTypeResults": [],
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
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
        "externalAgentConfig/import/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(
        &metrics,
        "externalAgentConfig/import/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_multi_worker_skills_changed_until_refresh() {
    let worker_a =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-a", vec!["skill-a"])
            .await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
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
        "skills/changed",
        serde_json::json!({}),
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(Duration::from_secs(1), websocket.next())
            .await
            .expect("expected skills/list response or refreshed notification")
            .expect("websocket message should exist")
            .expect("websocket message should decode");
        let Message::Text(text) = message else {
            continue;
        };
        let parsed =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("json-rpc message should decode");
        match parsed {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after skills/list refresh: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(post_refresh_duplicate.is_err(), true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_keeps_multi_worker_skills_changed_pending_after_failed_refresh() {
    let worker_a = start_mock_remote_server_for_skills_changed_and_failing_list().await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
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
        "skills/changed",
        serde_json::json!({}),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("failed-skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reopens_multi_worker_skills_changed_after_failed_then_successful_refresh()
 {
    let worker_a = start_mock_remote_server_for_skills_changed_failing_then_successful_list().await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
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
        "skills/changed",
        serde_json::json!({}),
    );

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("failed-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;
    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(duplicate.is_err());

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("successful-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(
            Duration::from_secs(1),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("skills/list response or refreshed notification should arrive");
        match message {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("successful-skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after successful skills/list: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(post_refresh_duplicate.is_err());

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_routes_multi_worker_plugin_management_to_first_successful_worker() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
        (
            "gitDiffToRemote",
            serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            }),
            serde_json::json!({
                "sha": "0123456789abcdef0123456789abcdef01234567",
                "diff": "diff --git a/README.md b/README.md\n",
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: crate::northbound::v2::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            expected_result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
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
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("plugin management request should serialize")
                .into(),
            ))
            .await
            .expect("plugin management request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected plugin management response for {method}");
        };
        assert_eq!(response.id, RequestId::String(format!("{method}-request")));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_chatgpt_auth_tokens_refresh_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("refresh-request-1".to_string()),
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    params: Some(serde_json::json!({
                        "reason": "unauthorized",
                        "previousAccountId": "acct-123",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("refresh response should exist")
            .expect("refresh response should decode")
        else {
            panic!("expected refresh response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("refresh response should decode")
        else {
            panic!("expected refresh response");
        };
        assert_eq!(
            response.id,
            RequestId::String("refresh-request-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "accessToken": "access-token-1",
                "chatgptAccountId": "acct-123",
                "chatgptPlanType": "pro",
            })
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
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

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded refresh request");
    };
    assert_eq!(
        request.id,
        RequestId::String("refresh-request-1".to_string())
    );
    assert_eq!(request.method, "account/chatgptAuthTokens/refresh");
    assert_json_params_eq(
        request.params,
        Some(serde_json::json!({
            "reason": "unauthorized",
            "previousAccountId": "acct-123",
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "accessToken": "access-token-1",
                    "chatgptAccountId": "acct-123",
                    "chatgptPlanType": "pro",
                }),
            }))
            .expect("refresh response should serialize")
            .into(),
        ))
        .await
        .expect("refresh response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_exec_command_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-exec-approval-1".to_string()),
                    method: "execCommandApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "approvalId": "approval-visible",
                        "command": ["echo", "hello"],
                        "cwd": "/tmp/workspace",
                        "reason": "Need to run a visible command",
                        "parsedCmd": [{
                            "type": "unknown",
                            "cmd": "echo hello",
                        }],
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-exec-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ExecCommandApprovalResponse {
                decision: ReviewDecision::Approved,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-exec-approval-1".to_string()),
                result: serde_json::to_value(ExecCommandApprovalResponse {
                    decision: ReviewDecision::Approved,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_apply_patch_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-patch-approval-1".to_string()),
                    method: "applyPatchApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "fileChanges": {
                            "README.md": {
                                "changeType": "added",
                                "oldContent": null,
                                "newContent": "hello\\n",
                            },
                        },
                        "reason": "Need to write visible changes",
                        "grantRoot": "/tmp/workspace",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-patch-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ApplyPatchApprovalResponse {
                decision: ReviewDecision::ApprovedForSession,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-patch-approval-1".to_string()),
                result: serde_json::to_value(ApplyPatchApprovalResponse {
                    decision: ReviewDecision::ApprovedForSession,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fails_closed_when_server_request_answer_targets_exhausted_account() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (downstream_answer_tx, downstream_answer_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let thread_start_request = read_websocket_request(&mut websocket).await;
        assert_eq!(thread_start_request.method, "thread/start");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: thread_start_request.id,
                    result: serde_json::json!({
                        "thread": {
                            "id": "thread-worker-a",
                            "forkedFromId": null,
                            "preview": "/tmp/worker-a",
                            "ephemeral": true,
                            "modelProvider": "openai",
                            "createdAt": 1,
                            "updatedAt": 1,
                            "status": {
                                "type": "idle",
                            },
                            "path": null,
                            "cwd": "/tmp/worker-a",
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
                        "cwd": "/tmp/worker-a",
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
                    id: RequestId::String("srv-user-input".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-worker-a",
                        "turnId": "turn-worker-a",
                        "itemId": "tool-call-worker-a",
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
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let maybe_response = timeout(Duration::from_millis(300), websocket.next())
            .await
            .ok()
            .and_then(|frame| frame)
            .and_then(Result::ok)
            .and_then(|message| match message {
                Message::Text(text) => Some(text.to_string()),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
                Message::Close(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => None,
            });
        downstream_answer_tx
            .send(maybe_response)
            .expect("downstream answer observation should send");
    });

    let worker_b = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            format!("ws://{downstream_addr}"),
            Some("acct-a".to_string()),
        ),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(
            GatewayV2SessionFactory::remote_multi_with_account_ids(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: format!("ws://{downstream_addr}"),
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
                vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
            )
            .with_worker_health(worker_health.clone()),
        )),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    send_initialized(&mut websocket).await;
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-start".to_string()),
                method: "thread/start".to_string(),
                params: Some(serde_json::json!({
                    "model": null,
                    "modelProvider": null,
                    "serviceTier": null,
                    "cwd": "/tmp/worker-a",
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "sandbox": null,
                    "config": null,
                    "serviceName": null,
                    "baseInstructions": null,
                    "developerInstructions": null,
                    "personality": null,
                    "ephemeral": true,
                    "sessionStartSource": null,
                    "dynamicTools": null,
                    "experimentalRawEvents": false,
                    "persistExtendedHistory": false,
                })),
                trace: None,
            }))
            .expect("thread/start request should serialize")
            .into(),
        ))
        .await
        .expect("thread/start request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread/start response");
    };
    assert_eq!(response.id, RequestId::String("thread-start".to_string()));

    let JSONRPCMessage::Request(user_input_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded user-input request");
    };
    assert_eq!(user_input_request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        user_input_request.params,
        Some(serde_json::json!({
            "threadId": "thread-worker-a",
            "turnId": "turn-worker-a",
            "itemId": "tool-call-worker-a",
            "questions": [{
                "id": "mode",
                "header": "Mode",
                "question": "Pick execution mode",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    worker_health.mark_account_exhausted_for_worker(0, "quota reached".to_string());
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: user_input_request.id,
                result: serde_json::json!({
                    "answers": {
                        "mode": {
                            "answers": ["safe"],
                        },
                    },
                }),
            }))
            .expect("user-input response should serialize")
            .into(),
        ))
        .await
        .expect("user-input response should send");

    match timeout(Duration::from_secs(2), websocket.next())
        .await
        .expect("websocket should close or reset")
    {
        Some(Ok(Message::Close(Some(close_frame)))) => {
            assert_eq!(u16::from(close_frame.code), close_code::ERROR);
            assert!(
                    close_frame.reason.contains(
                        "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
                    ),
                    "{}",
                    close_frame.reason
                );
        }
        Some(Ok(Message::Close(None))) | None => {}
        Some(Err(err)) => {
            assert!(
                err.to_string().contains("reset without closing handshake"),
                "{err}"
            );
        }
        Some(Ok(message)) => panic!("expected websocket close or reset, got {message:?}"),
    }
    assert_eq!(
        downstream_answer_rx
            .await
            .expect("downstream answer observation should arrive"),
        None
    );
    assert_v2_server_request_answer_account_exhaustion_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "response", 1),
            (
                "downstream_server_request_forwarded",
                "item/tool/requestUserInput",
                1,
            ),
        ],
        &[("response", 1)],
        &[(0, "active_thread_handoff_failure", 1)],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-a"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "default",
            "projectId": null,
            "method": "serverRequest/respond",
            "threadId": "thread-worker-a",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
        })
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("active_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("active_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("default")
    );
    assert_eq!(health.last_account_capacity_event_project_id, None);
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);

    server_task.abort();
    let _ = server_task.await;
}

#[path = "v2_tests_cases_3_late_reconnect.rs"]
mod v2_tests_cases_3_late_reconnect;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_3_late_reconnect::*;
