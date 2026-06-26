use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_that_sends_binary_after_initialize() -> String {
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
                    id: request.id,
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
        websocket
            .send(Message::Binary(vec![0, 1, 2].into()))
            .await
            .expect("binary frame should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_unexpected_response_after_initialize()
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
                    id: request.id,
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
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: RequestId::String("unexpected-response".to_string()),
                    result: serde_json::json!({}),
                }))
                .expect("unexpected response should serialize")
                .into(),
            ))
            .await
            .expect("unexpected response should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_unexpected_error_after_initialize() -> String
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
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
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
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: RequestId::String("unexpected-error".to_string()),
                    error: JSONRPCErrorError {
                        code: -32603,
                        message: "unexpected".to_string(),
                        data: None,
                    },
                }))
                .expect("unexpected error should serialize")
                .into(),
            ))
            .await
            .expect("unexpected error should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_disconnects_after_initialize_with_reason(
    reason: String,
) -> String {
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
            .close((!reason.is_empty()).then_some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Error,
                    reason: reason.into(),
                },
            ))
            .await
            .expect("close frame should send");
    });
    format!("ws://{addr}")
}

pub(crate) async fn send_remote_notification(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    method: &str,
    params: serde_json::Value,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: method.to_string(),
                params: Some(params),
            }))
            .expect("notification should serialize")
            .into(),
        ))
        .await
        .expect("notification should send");
}

pub(crate) fn assert_jsonrpc_notification(
    message: JSONRPCMessage,
    expected_method: &str,
    expected_params: serde_json::Value,
) {
    let JSONRPCMessage::Notification(notification) = message else {
        panic!("expected notification");
    };
    assert_eq!(notification.method, expected_method);
    assert_json_params_eq(notification.params, Some(expected_params));
}

pub(crate) async fn expect_remote_initialize(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
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
                id: request.id,
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
}

pub(crate) async fn expect_remote_initialize_split<S, R>(write: &mut S, read: &mut R)
where
    S: futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    R: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let frame = read
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
    write
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }))
            .expect("initialize response should serialize")
            .into(),
        ))
        .await
        .expect("initialize response should send");

    let frame = read
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
}
