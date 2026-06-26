use super::*;

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
