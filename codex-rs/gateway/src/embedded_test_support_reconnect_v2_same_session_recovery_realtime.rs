use super::*;

pub(crate) async fn handle_same_session_recovery_realtime_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    thread_id: &'static str,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> bool {
    match request.method.as_str() {
        "thread/realtime/start" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/realtime/start should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/started".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "realtimeSessionId": format!("session-{thread_id}"),
                        "version": "v2",
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/itemAdded".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "item": {
                            "type": "message",
                            "id": format!("item-{thread_id}"),
                        },
                    })),
                }),
            )
            .await;
            true
        }
        "thread/realtime/appendText" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/realtime/appendText should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/transcript/delta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "role": "assistant",
                        "delta": format!("delta {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/transcript/done".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "role": "assistant",
                        "text": format!("done {thread_id}"),
                    })),
                }),
            )
            .await;
            true
        }
        "thread/realtime/appendAudio" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/realtime/appendAudio should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/outputAudio/delta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "audio": {
                            "data": request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("audio"))
                                .and_then(|audio| audio.get("data"))
                                .cloned()
                                .expect("thread/realtime/appendAudio should include audio.data"),
                            "sampleRate": 24_000,
                            "numChannels": 1,
                            "samplesPerChannel": 3,
                            "itemId": request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("audio"))
                                .and_then(|audio| audio.get("itemId"))
                                .cloned()
                                .expect("thread/realtime/appendAudio should include audio.itemId"),
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/sdp".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Codex\r\n",
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/error".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "message": "realtime transport warning",
                    })),
                }),
            )
            .await;
            true
        }
        "thread/realtime/stop" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/realtime/stop should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/realtime/closed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "reason": "client requested stop",
                    })),
                }),
            )
            .await;
            true
        }
        _ => false,
    }
}
