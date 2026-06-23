use super::*;

pub(crate) async fn handle_same_session_recovery_thread_controls_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    thread_id: &'static str,
    preview: &str,
    request: &codex_app_server_protocol::JSONRPCRequest,
    thread_name: &mut Option<String>,
    turn_id: &str,
) -> bool {
    match request.method.as_str() {
        "thread/name/set" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/name/set should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            let name = request
                .params
                .as_ref()
                .and_then(|params| params.get("name"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/name/set should include name");
            thread_name.replace(name.to_string());
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
                    method: "thread/name/updated".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "threadName": name,
                    })),
                }),
            )
            .await;
            true
        }
        "thread/memoryMode/set" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/memoryMode/set should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            true
        }
        "thread/unsubscribe" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/unsubscribe should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "status": "unsubscribed",
                    }),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/closed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                    })),
                }),
            )
            .await;
            true
        }
        "thread/archive" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/archive should include threadId");
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
                    method: "thread/archived".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                    })),
                }),
            )
            .await;
            true
        }
        "thread/unarchive" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/unarchive should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": mock_thread_with_name(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                        ),
                    }),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/unarchived".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                    })),
                }),
            )
            .await;
            true
        }
        "thread/metadata/update" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/metadata/update should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": mock_thread_with_name(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                        ),
                    }),
                }),
            )
            .await;
            true
        }
        "thread/turns/list" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/turns/list should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [mock_turn(turn_id, "completed")],
                        "nextCursor": null,
                        "backwardsCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/increment_elicitation" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/increment_elicitation should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "count": 1,
                        "paused": true,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/decrement_elicitation" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/decrement_elicitation should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "count": 0,
                        "paused": false,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/inject_items" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/inject_items should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            true
        }
        "thread/compact/start" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/compact/start should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            true
        }
        "thread/shellCommand" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/shellCommand should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            true
        }
        "thread/backgroundTerminals/clean" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/backgroundTerminals/clean should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({}),
                }),
            )
            .await;
            true
        }
        "thread/rollback" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/rollback should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": mock_thread_with_name(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                        ),
                    }),
                }),
            )
            .await;
            true
        }
        _ => false,
    }
}
