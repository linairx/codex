use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn spawn_remote_gateway_v2_test_server(
    websocket_url: String,
    scope_registry: Arc<GatewayScopeRegistry>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let initialize_response = test_initialize_response().await;
    spawn_test_server(GatewayV2State {
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
    .await
}

pub(crate) async fn start_mock_remote_server_that_disconnects_after_initialize() -> String {
    start_mock_remote_server_that_disconnects_after_initialize_with_reason(String::new()).await
}

pub(crate) async fn start_mock_remote_server_that_stays_connected_after_initialize() -> String {
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
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_invalid_jsonrpc_after_initialize() -> String
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
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid JSON-RPC should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_binary_during_initialize() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
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
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Binary(vec![0, 1, 2].into()))
            .await
            .expect("binary frame should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_server_request_during_initialize()
-> (String, oneshot::Receiver<JSONRPCResponse>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (response_tx, response_rx) = oneshot::channel();
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

        let server_request_id = RequestId::String("srv-init".to_string());
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: server_request_id.clone(),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-init",
                        "turnId": "turn-init",
                        "itemId": "item-init",
                        "questions": [{
                            "id": "question-init",
                            "header": "Mode",
                            "question": "Pick one",
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

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected server request response");
        };
        assert_eq!(response.id, server_request_id);
        response_tx
            .send(response)
            .expect("test should wait for server request response");
    });
    (format!("ws://{addr}"), response_rx)
}

pub(crate) async fn start_mock_remote_server_that_sends_unknown_server_request_during_initialize()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
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

        let server_request_id = RequestId::String("srv-init-unknown".to_string());
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: server_request_id.clone(),
                    method: "thread/unknown".to_string(),
                    params: None,
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected setup-time server request rejection");
        };
        assert_eq!(error.id, server_request_id);
        assert_eq!(error.error.code, -32601);
        assert_eq!(
            error.error.message,
            "unsupported remote app-server request `thread/unknown`"
        );

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

        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected follow-up request");
        };
        assert_eq!(request.method, "model/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({ "models": [] }),
                }))
                .expect("model list response should serialize")
                .into(),
            ))
            .await
            .expect("model list response should send");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_invalid_jsonrpc_during_initialize() -> String
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

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid JSON-RPC should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_wrong_id_response_during_initialize()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
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
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: RequestId::String("not-initialize".to_string()),
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("wrong-id initialize response should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_wrong_id_error_during_initialize() -> String
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

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: RequestId::String("not-initialize".to_string()),
                    error: JSONRPCErrorError {
                        code: -32603,
                        message: "unexpected initialize error".to_string(),
                        data: None,
                    },
                }))
                .expect("initialize error should serialize")
                .into(),
            ))
            .await
            .expect("wrong-id initialize error should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}
