use super::*;

pub(crate) async fn handle_same_session_recovery_discovery_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    context: &SameSessionRecoveryContext,
    request: &codex_app_server_protocol::JSONRPCRequest,
    thread_name: &mut Option<String>,
) -> bool {
    let thread_id = context.thread_id;
    let preview = context.preview;
    match request.method.as_str() {
        "configRequirements/read" => {
            assert_eq!(request.params, None);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "requirements": null,
                    }),
                }),
            )
            .await;
            true
        }
        "app/list" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("app/list should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [{
                            "id": format!("{thread_id}-app"),
                            "name": format!("{thread_id} app"),
                            "description": format!("{thread_id} connector"),
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
                        "nextCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "mcpServer/resource/read" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("mcpServer/resource/read should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            let uri = request
                .params
                .as_ref()
                .and_then(|params| params.get("uri"))
                .and_then(serde_json::Value::as_str)
                .expect("mcpServer/resource/read should include uri");
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "text/markdown",
                            "text": format!("{thread_id} recovered resource"),
                        }],
                    }),
                }),
            )
            .await;
            true
        }
        "mcpServer/tool/call" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("mcpServer/tool/call should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            let tool = request
                .params
                .as_ref()
                .and_then(|params| params.get("tool"))
                .and_then(serde_json::Value::as_str)
                .expect("mcpServer/tool/call should include tool");
            assert_eq!(tool, "lookup");
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("{thread_id} recovered tool result"),
                        }],
                        "structuredContent": {
                            "threadId": thread_id,
                        },
                        "isError": false,
                    }),
                }),
            )
            .await;
            true
        }
        "review/start" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("review/start should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "turn": mock_turn(&context.review_turn_id, "inProgress"),
                        "reviewThreadId": context.review_thread_id.as_str(),
                    }),
                }),
            )
            .await;
            true
        }
        "thread/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [mock_thread_with_name(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                        )],
                        "nextCursor": null,
                        "backwardsCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/loaded/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [thread_id],
                        "nextCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        _ => false,
    }
}
