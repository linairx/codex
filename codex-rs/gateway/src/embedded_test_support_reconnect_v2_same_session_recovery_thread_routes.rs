use super::*;

pub(crate) async fn handle_same_session_recovery_thread_routes_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    context: &SameSessionRecoveryContext,
    request: &codex_app_server_protocol::JSONRPCRequest,
    thread_name: &mut Option<String>,
) -> bool {
    let thread_id = context.thread_id;
    let preview = context.preview;
    match request.method.as_str() {
        "thread/start" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": mock_thread_with_name_and_path(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                            Some(&context.rollout_path),
                        ),
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": preview,
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess"
                        },
                        "reasoningEffort": null,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/read" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/read should include threadId");
            assert!(
                requested_thread_id == thread_id
                    || requested_thread_id == context.review_thread_id
                    || requested_thread_id == context.forked_thread_id,
                "thread/read should target the recovered thread, its fork, or its review thread",
            );
            let thread = if requested_thread_id == thread_id {
                mock_thread_with_name_and_path(
                    thread_id,
                    preview,
                    thread_name.as_deref(),
                    Some(&context.rollout_path),
                )
            } else if requested_thread_id == context.forked_thread_id {
                mock_thread_with_path(
                    &context.forked_thread_id,
                    &context.forked_preview,
                    Some(&context.forked_rollout_path),
                )
            } else {
                mock_thread_with_path(
                    &context.review_thread_id,
                    preview,
                    Some(&context.rollout_path),
                )
            };
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": thread,
                        "turns": [],
                    }),
                }),
            )
            .await;
            true
        }
        "thread/resume" => {
            let requested_path = request
                .params
                .as_ref()
                .and_then(|params| params.get("path"))
                .and_then(serde_json::Value::as_str);
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/resume should include threadId");
            if let Some(requested_path) = requested_path {
                assert_eq!(
                    requested_path,
                    context.rollout_path.as_str(),
                    "path-based thread/resume should stay on the recovered worker",
                );
            } else {
                assert_eq!(requested_thread_id, thread_id);
            }
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": mock_thread_with_name_and_path(
                            thread_id,
                            preview,
                            thread_name.as_deref(),
                            Some(&context.rollout_path),
                        ),
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": preview,
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess"
                        },
                        "reasoningEffort": null,
                    }),
                }),
            )
            .await;
            true
        }
        "thread/fork" => {
            let requested_path = request
                .params
                .as_ref()
                .and_then(|params| params.get("path"))
                .and_then(serde_json::Value::as_str);
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("thread/fork should include threadId");
            if let Some(requested_path) = requested_path {
                assert_eq!(
                    requested_path,
                    context.rollout_path.as_str(),
                    "path-based thread/fork should stay on the recovered worker",
                );
            } else {
                assert_eq!(requested_thread_id, thread_id);
            }
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "thread": {
                            "id": context.forked_thread_id.as_str(),
                            "sessionId": context.forked_thread_id.as_str(),
                            "forkedFromId": thread_id,
                            "preview": context.forked_preview.as_str(),
                            "ephemeral": true,
                            "modelProvider": "openai",
                            "createdAt": 1,
                            "updatedAt": 1,
                            "status": {
                                "type": "idle"
                            },
                            "path": context.forked_rollout_path.as_str(),
                            "cwd": context.forked_preview.as_str(),
                            "cliVersion": "0.0.0-test",
                            "source": "vscode",
                            "agentNickname": null,
                            "agentRole": null,
                            "gitInfo": null,
                            "name": null,
                            "turns": [],
                        },
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": context.forked_preview.as_str(),
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess"
                        },
                        "reasoningEffort": null,
                    }),
                }),
            )
            .await;
            true
        }
        _ => false,
    }
}
