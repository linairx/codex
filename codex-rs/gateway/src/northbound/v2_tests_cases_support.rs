use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn send_initialize(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    send_initialize_with_capabilities(websocket, None).await;
}

pub(crate) async fn send_initialized(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("initialized notification should serialize")
            .into(),
        ))
        .await
        .expect("initialized notification should send");
}

pub(crate) async fn send_jsonrpc_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id,
                method: method.to_string(),
                params: Some(params),
                trace: None,
            }))
            .expect("json-rpc request should serialize")
            .into(),
        ))
        .await
        .expect("json-rpc request should send");
}

pub(crate) async fn send_initialize_with_capabilities(
    websocket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    capabilities: Option<InitializeCapabilities>,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("initialize request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(websocket).await else {
        panic!("expected initialize response");
    };
    assert_eq!(response.id, RequestId::String("initialize".to_string()));
}
