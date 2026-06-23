use super::*;

pub(crate) async fn handle_same_session_recovery_turn_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    thread_id: &'static str,
    request: &codex_app_server_protocol::JSONRPCRequest,
    turn_id: &str,
) -> bool {
    match request.method.as_str() {
        "turn/start" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("turn/start should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "turn": mock_turn(turn_id, "inProgress"),
                    }),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/status/changed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "status": {
                            "type": "active",
                            "activeFlags": [],
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "turn/started".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turn": mock_turn(turn_id, "inProgress"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "hook/started".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "run": {
                            "id": format!("hook-{thread_id}"),
                            "eventName": "userPromptSubmit",
                            "handlerType": "command",
                            "executionMode": "sync",
                            "scope": "turn",
                            "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                            "source": "user",
                            "displayOrder": 0,
                            "status": "running",
                            "statusMessage": format!("hook started {thread_id}"),
                            "startedAt": 1,
                            "completedAt": null,
                            "durationMs": null,
                            "entries": [],
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/started".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "startedAtMs": 0,
                        "item": {
                            "type": "agentMessage",
                            "id": format!("msg-{thread_id}"),
                            "text": "streaming answer in progress",
                            "phase": "commentary",
                            "memoryCitation": null,
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/agentMessage/delta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("msg-{thread_id}"),
                        "delta": "hello from recovered worker",
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/plan/delta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("plan-{thread_id}"),
                        "delta": format!("plan {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/reasoning/summaryTextDelta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("reasoning-summary-{thread_id}"),
                        "delta": format!("summary {thread_id}"),
                        "summaryIndex": 0,
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/reasoning/summaryPartAdded".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("reasoning-summary-{thread_id}"),
                        "summaryIndex": 0,
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/reasoning/textDelta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("reasoning-{thread_id}"),
                        "delta": format!("reasoning {thread_id}"),
                        "contentIndex": 0,
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/commandExecution/terminalInteraction".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("terminal-{thread_id}"),
                        "processId": format!("proc-{thread_id}"),
                        "stdin": "y\n",
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/commandExecution/outputDelta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("exec-{thread_id}"),
                        "delta": format!("stdout {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/fileChange/outputDelta".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("patch-{thread_id}"),
                        "delta": format!("patch {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "turn/diff/updated".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "diff": format!("diff {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "turn/plan/updated".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "explanation": "gateway multi-worker plan",
                        "plan": [{
                            "step": format!("plan {thread_id}"),
                            "status": "completed",
                        }],
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/tokenUsage/updated".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "tokenUsage": {
                            "total": {
                                "totalTokens": 42,
                                "inputTokens": 20,
                                "cachedInputTokens": 3,
                                "outputTokens": 22,
                                "reasoningOutputTokens": 7,
                            },
                            "last": {
                                "totalTokens": 10,
                                "inputTokens": 4,
                                "cachedInputTokens": 1,
                                "outputTokens": 6,
                                "reasoningOutputTokens": 2,
                            },
                            "modelContextWindow": 200000,
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/mcpToolCall/progress".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "itemId": format!("mcp-{thread_id}"),
                        "message": format!("mcp progress {thread_id}"),
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "thread/compacted".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "model/rerouted".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "fromModel": "gpt-5",
                        "toModel": "gpt-5-codex",
                        "reason": "highRiskCyberActivity",
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "rawResponseItem/completed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "item": {
                            "type": "other",
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "error".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "willRetry": false,
                        "error": {
                            "message": format!("recoverable warning {thread_id}"),
                            "codexErrorInfo": null,
                            "additionalDetails": "gateway multi-worker warning",
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/autoApprovalReview/started".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "startedAtMs": 0,
                        "reviewId": format!("guardian-{thread_id}"),
                        "targetItemId": format!("cmd-{thread_id}"),
                        "review": {
                            "status": "inProgress",
                            "riskLevel": null,
                            "userAuthorization": null,
                            "rationale": null,
                        },
                        "action": {
                            "type": "command",
                            "source": "shell",
                            "command": "cat docs/gateway.md",
                            "cwd": "/tmp",
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/autoApprovalReview/completed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "startedAtMs": 0,
                        "completedAtMs": 0,
                        "reviewId": format!("guardian-{thread_id}"),
                        "targetItemId": format!("cmd-{thread_id}"),
                        "decisionSource": "agent",
                        "review": {
                            "status": "approved",
                            "riskLevel": "low",
                            "userAuthorization": "low",
                            "rationale": "Read-only command.",
                        },
                        "action": {
                            "type": "command",
                            "source": "shell",
                            "command": "cat docs/gateway.md",
                            "cwd": "/tmp",
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "hook/completed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "run": {
                            "id": format!("hook-{thread_id}"),
                            "eventName": "userPromptSubmit",
                            "handlerType": "command",
                            "executionMode": "sync",
                            "scope": "turn",
                            "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                            "source": "user",
                            "displayOrder": 0,
                            "status": "completed",
                            "statusMessage": format!("hook completed {thread_id}"),
                            "startedAt": 1,
                            "completedAt": 2,
                            "durationMs": 1,
                            "entries": [],
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "item/completed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "completedAtMs": 0,
                        "item": {
                            "type": "agentMessage",
                            "id": format!("msg-{thread_id}"),
                            "text": "streaming answer completed",
                            "phase": "final_answer",
                            "memoryCitation": null,
                        },
                    })),
                }),
            )
            .await;
            write_websocket_message(
                websocket,
                JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                    method: "turn/completed".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": thread_id,
                        "turn": mock_turn(turn_id, "completed"),
                    })),
                }),
            )
            .await;
            true
        }
        "turn/steer" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("turn/steer should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            let expected_turn_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("expectedTurnId"))
                .and_then(serde_json::Value::as_str)
                .expect("turn/steer should include expectedTurnId");
            assert_eq!(expected_turn_id, turn_id);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "turnId": turn_id,
                    }),
                }),
            )
            .await;
            true
        }
        "turn/interrupt" => {
            let requested_thread_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("threadId"))
                .and_then(serde_json::Value::as_str)
                .expect("turn/interrupt should include threadId");
            assert_eq!(requested_thread_id, thread_id);
            let requested_turn_id = request
                .params
                .as_ref()
                .and_then(|params| params.get("turnId"))
                .and_then(serde_json::Value::as_str)
                .expect("turn/interrupt should include turnId");
            assert_eq!(requested_turn_id, turn_id);
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
        _ => false,
    }
}
