use super::*;

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
