use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_realtime_started_notifications_for_visible_threads() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_realtime_started_notification().await;
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

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "thread/realtime/started",
        serde_json::json!({
            "threadId": "thread-visible",
            "realtimeSessionId": "realtime-session-1",
            "version": "v1",
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_additional_realtime_notifications_for_visible_threads() {
    let cases = vec![
        (
            "thread/realtime/itemAdded",
            serde_json::json!({
                "threadId": "thread-visible",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [],
                },
            }),
        ),
        (
            "thread/realtime/outputAudio/delta",
            serde_json::json!({
                "threadId": "thread-visible",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-visible",
                },
            }),
        ),
        (
            "thread/realtime/transcript/delta",
            serde_json::json!({
                "threadId": "thread-visible",
                "role": "assistant",
                "delta": "hello from realtime transcript",
            }),
        ),
        (
            "thread/realtime/transcript/done",
            serde_json::json!({
                "threadId": "thread-visible",
                "role": "assistant",
                "text": "hello from realtime transcript",
            }),
        ),
        (
            "thread/realtime/sdp",
            serde_json::json!({
                "threadId": "thread-visible",
                "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Codex\r\n",
            }),
        ),
        (
            "thread/realtime/error",
            serde_json::json!({
                "threadId": "thread-visible",
                "message": "realtime transport failed",
            }),
        ),
        (
            "thread/realtime/closed",
            serde_json::json!({
                "threadId": "thread-visible",
                "reason": "client-requested",
            }),
        ),
    ];

    for (method, params) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url =
            start_mock_remote_server_for_realtime_notification(method, params.clone()).await;
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

        assert_jsonrpc_notification(read_websocket_message(&mut websocket).await, method, params);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_fans_in_multi_worker_realtime_notifications_for_visible_threads() {
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        (
            "thread/realtime/started",
            serde_json::json!({
                "threadId": "thread-worker-a",
                "realtimeSessionId": "session-worker-a",
                "version": "v1",
            }),
        ),
        (
            "thread/realtime/itemAdded",
            serde_json::json!({
                "threadId": "thread-worker-a",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [],
                },
            }),
        ),
        (
            "thread/realtime/transcript/delta",
            serde_json::json!({
                "threadId": "thread-worker-a",
                "role": "assistant",
                "delta": "worker a transcript",
            }),
        ),
        (
            "thread/realtime/transcript/done",
            serde_json::json!({
                "threadId": "thread-worker-a",
                "role": "assistant",
                "text": "worker a transcript complete",
            }),
        ),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications(vec![
        (
            "thread/realtime/outputAudio/delta",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-worker-b",
                },
            }),
        ),
        (
            "thread/realtime/sdp",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Worker B\r\n",
            }),
        ),
        (
            "thread/realtime/error",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "message": "worker b realtime warning",
            }),
        ),
        (
            "thread/realtime/closed",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "reason": "worker-b-closed",
            }),
        ),
    ])
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
    scope_registry.register_thread("thread-worker-b".to_string(), context);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
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

    let mut forwarded = Vec::new();
    for _ in 0..8 {
        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected realtime notification");
        };
        forwarded.push((notification.method, notification.params));
    }
    forwarded.sort_by(|left, right| left.0.cmp(&right.0));

    assert_eq!(
        forwarded,
        vec![
            (
                "thread/realtime/closed".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "reason": "worker-b-closed",
                })),
            ),
            (
                "thread/realtime/error".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "message": "worker b realtime warning",
                })),
            ),
            (
                "thread/realtime/itemAdded".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-a",
                    "item": {
                        "type": "message",
                        "role": "assistant",
                        "status": "completed",
                        "content": [],
                    },
                })),
            ),
            (
                "thread/realtime/outputAudio/delta".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "audio": {
                        "data": "AQID",
                        "sampleRate": 24000,
                        "numChannels": 1,
                        "samplesPerChannel": 3,
                        "itemId": "item-worker-b",
                    },
                })),
            ),
            (
                "thread/realtime/sdp".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Worker B\r\n",
                })),
            ),
            (
                "thread/realtime/started".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-a",
                    "realtimeSessionId": "session-worker-a",
                    "version": "v1",
                })),
            ),
            (
                "thread/realtime/transcript/delta".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-a",
                    "role": "assistant",
                    "delta": "worker a transcript",
                })),
            ),
            (
                "thread/realtime/transcript/done".to_string(),
                Some(serde_json::json!({
                    "threadId": "thread-worker-a",
                    "role": "assistant",
                    "text": "worker a transcript complete",
                })),
            ),
        ]
    );

    server_task.abort();
    let _ = server_task.await;
}
