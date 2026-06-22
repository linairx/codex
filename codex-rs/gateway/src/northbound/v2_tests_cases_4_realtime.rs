use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_realtime_start_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_realtime_start().await;
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

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("realtime-start".to_string()),
                method: "thread/realtime/start".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "outputModality": "text",
                    "transport": {
                        "type": "websocket"
                    }
                })),
                trace: None,
            }))
            .expect("realtime start request should serialize")
            .into(),
        ))
        .await
        .expect("realtime start request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected realtime start response");
    };
    assert_eq!(response.id, RequestId::String("realtime-start".to_string()));
    assert_eq!(response.result, serde_json::json!({}));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_realtime_append_text_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_realtime_append_text().await;
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

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("realtime-append-text".to_string()),
                method: "thread/realtime/appendText".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "text": "hello realtime",
                })),
                trace: None,
            }))
            .expect("realtime append text request should serialize")
            .into(),
        ))
        .await
        .expect("realtime append text request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected realtime append text response");
    };
    assert_eq!(
        response.id,
        RequestId::String("realtime-append-text".to_string())
    );
    assert_eq!(response.result, serde_json::json!({}));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_realtime_append_audio_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_realtime_append_audio().await;
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

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("realtime-append-audio".to_string()),
                method: "thread/realtime/appendAudio".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "audio": {
                        "data": "AQID",
                        "sampleRate": 24000,
                        "numChannels": 1,
                        "samplesPerChannel": 3,
                        "itemId": "item-visible",
                    }
                })),
                trace: None,
            }))
            .expect("realtime append audio request should serialize")
            .into(),
        ))
        .await
        .expect("realtime append audio request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected realtime append audio response");
    };
    assert_eq!(
        response.id,
        RequestId::String("realtime-append-audio".to_string())
    );
    assert_eq!(response.result, serde_json::json!({}));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_realtime_stop_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_realtime_stop().await;
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

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("realtime-stop".to_string()),
                method: "thread/realtime/stop".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                })),
                trace: None,
            }))
            .expect("realtime stop request should serialize")
            .into(),
        ))
        .await
        .expect("realtime stop request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected realtime stop response");
    };
    assert_eq!(response.id, RequestId::String("realtime-stop".to_string()));
    assert_eq!(response.result, serde_json::json!({}));

    server_task.abort();
    let _ = server_task.await;
}
