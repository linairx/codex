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
