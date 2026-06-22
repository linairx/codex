use super::*;

pub(crate) async fn start_mock_remote_multi_connection_workflow_server(
    thread_id: &'static str,
    preview: &'static str,
    turn_id: &'static str,
    delta: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;
                let review_thread_id = format!("{thread_id}-review");
                let review_turn_id = format!("turn-review-{thread_id}");
                let review_preview = format!("{preview}/review");

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "configRequirements/read" => {
                            pretty_assertions::assert_eq!(request.params, None);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "requirements": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/start" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
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
                        }
                        "turn/start" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("turn/start params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turn": mock_turn(turn_id, "inProgress"),
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "status": {
                                                "type": "active",
                                                "activeFlags": [],
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(turn_id, "inProgress"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                                "statusMessage": format!("hook running {thread_id}"),
                                                "startedAt": 1,
                                                "completedAt": null,
                                                "durationMs": null,
                                                "entries": [],
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "startedAtMs": 0,
                                            "item": {
                                                "type": "agentMessage",
                                                "id": format!("msg-{thread_id}"),
                                                "text": "streaming answer in progress",
                                                "phase": "final_answer",
                                                "memoryCitation": null,
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/agentMessage/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("msg-{thread_id}"),
                                            "delta": delta,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/plan/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("plan-{thread_id}"),
                                            "delta": format!("plan {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryTextDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("summary-{thread_id}"),
                                            "delta": format!("summary {thread_id}"),
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryPartAdded".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("summary-{thread_id}"),
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-{thread_id}"),
                                            "delta": format!("reasoning {thread_id}"),
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/terminalInteraction"
                                            .to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("terminal-{thread_id}"),
                                            "processId": format!("proc-{thread_id}"),
                                            "stdin": "y\n",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("exec-{thread_id}"),
                                            "delta": format!("stdout {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("patch-{thread_id}"),
                                            "delta": format!("patch {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/diff/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "diff": format!("diff {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/mcpToolCall/progress".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("mcp-{thread_id}"),
                                            "message": format!("mcp progress {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/compacted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "model/rerouted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "fromModel": "gpt-5",
                                            "toModel": "gpt-5-codex",
                                            "reason": "highRiskCyberActivity",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "rawResponseItem/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "item": {
                                                "type": "other",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(turn_id, "completed"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            if requested_thread_id == review_thread_id {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": mock_thread(&review_thread_id, &review_preview),
                                            "model": "gpt-5",
                                            "modelProvider": "openai",
                                            "serviceTier": null,
                                            "cwd": review_preview,
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
                            } else {
                                pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": mock_thread(thread_id, preview),
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
                            }
                        }
                        "thread/unsubscribe" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/unsubscribe should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "status": "unsubscribed",
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/closed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/archive" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/archive should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/archived".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/unarchive" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/unarchive should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/unarchived".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/metadata/update" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/metadata/update should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/turns/list" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/turns/list should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [mock_turn(turn_id, "completed")],
                                        "nextCursor": null,
                                        "backwardsCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/increment_elicitation" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/increment_elicitation should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "count": 1,
                                        "paused": true,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/decrement_elicitation" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/decrement_elicitation should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "count": 0,
                                        "paused": false,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/inject_items" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/inject_items should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                        }
                        "thread/compact/start"
                        | "thread/shellCommand"
                        | "thread/backgroundTerminals/clean" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread control request should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                        }
                        "thread/rollback" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/rollback should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
                                    }),
                                }),
                            )
                            .await;
                        }
                        "review/start" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("review/start should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turn": mock_turn(&review_turn_id, "inProgress"),
                                        "reviewThreadId": review_thread_id,
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(&review_turn_id, "inProgress"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "review/submit" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("review/submit should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turn": mock_turn(&review_turn_id, "completed"),
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(&review_turn_id, "completed"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "item/agentMessage/delta" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("item/agentMessage/delta should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            let requested_turn_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("turnId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("item/agentMessage/delta should include turnId");
                            pretty_assertions::assert_eq!(requested_turn_id, turn_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/agentMessage/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("msg-{thread_id}"),
                                            "delta": delta,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "item/reasoning/textDelta" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("item/reasoning/textDelta should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            let requested_turn_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("turnId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("item/reasoning/textDelta should include turnId");
                            pretty_assertions::assert_eq!(requested_turn_id, turn_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-{thread_id}"),
                                            "delta": format!("reasoning {thread_id}"),
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/terminalInteraction"
                                            .to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("terminal-{thread_id}"),
                                            "processId": format!("proc-{thread_id}"),
                                            "stdin": "y\n",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("exec-{thread_id}"),
                                            "delta": format!("stdout {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("patch-{thread_id}"),
                                            "delta": format!("patch {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/diff/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "diff": format!("diff {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/mcpToolCall/progress".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("mcp-{thread_id}"),
                                            "message": format!("mcp progress {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/compacted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "model/rerouted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "fromModel": "gpt-5",
                                            "toModel": "gpt-5-codex",
                                            "reason": "highRiskCyberActivity",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "rawResponseItem/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "item": {
                                                "type": "other",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
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
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(turn_id, "completed"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        method => panic!("unexpected request method: {method}"),
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_realtime_server(
    thread_id: &'static str,
    preview: &'static str,
    session_id: &'static str,
    transcript_delta: &'static str,
    transcript_done: &'static str,
) -> String {
    start_mock_remote_multi_connection_realtime_server_with_voices(
        thread_id,
        preview,
        session_id,
        transcript_delta,
        transcript_done,
        RealtimeVoicesList::builtin(),
    )
    .await
}

pub(crate) async fn start_mock_remote_multi_connection_realtime_server_with_voices(
    thread_id: &'static str,
    preview: &'static str,
    session_id: &'static str,
    transcript_delta: &'static str,
    transcript_done: &'static str,
    realtime_voices: RealtimeVoicesList,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let realtime_voices = realtime_voices.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "thread/start" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
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
                        }
                        "thread/realtime/listVoices" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "voices": serde_json::to_value(&realtime_voices)
                                            .expect("realtime voices should serialize"),
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/realtime/start" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("realtime start params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "realtimeSessionId": session_id,
                                            "version": "v2",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/itemAdded".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "item": {
                                                "type": "message",
                                                "id": format!("item-{thread_id}"),
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/realtime/appendText" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("realtime appendText params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/transcript/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "role": "assistant",
                                            "delta": transcript_delta,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/transcript/done".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "role": "assistant",
                                            "text": transcript_done,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/realtime/appendAudio" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("realtime appendAudio params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/outputAudio/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "audio": {
                                                "data": params["audio"]["data"].clone(),
                                                "sampleRate": 24000,
                                                "numChannels": 1,
                                                "samplesPerChannel": 3,
                                                "itemId": params["audio"]["itemId"].clone(),
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/sdp".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Codex\r\n",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/error".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "message": "realtime transport warning",
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "thread/realtime/stop" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("realtime stop params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/closed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "reason": "client requested stop",
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        _ => panic!("unexpected request method: {}", request.method),
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_turn_control_server(
    thread_id: &'static str,
    preview: &'static str,
    steer_delta: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                let turn_id = format!("turn-{thread_id}");

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "thread/start" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread(thread_id, preview),
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
                        }
                        "turn/start" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("turn/start params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turn": mock_turn(&turn_id, "inProgress"),
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "status": {
                                                "type": "active",
                                                "activeFlags": [],
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(&turn_id, "inProgress"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "turn/steer" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("turn/steer params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            pretty_assertions::assert_eq!(params["expectedTurnId"], turn_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turnId": turn_id,
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/agentMessage/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("msg-{thread_id}"),
                                            "delta": steer_delta,
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        "turn/interrupt" => {
                            let params = request
                                .params
                                .as_ref()
                                .expect("turn/interrupt params should exist");
                            pretty_assertions::assert_eq!(params["threadId"], thread_id);
                            pretty_assertions::assert_eq!(params["turnId"], turn_id);
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(&turn_id, "completed"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "status": {
                                                "type": "idle",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                        }
                        method => panic!("unexpected request method: {method}"),
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_plugin_server(
    marketplace_name: &'static str,
    preview: &'static str,
    plugin_name: &'static str,
    description: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let plugin_id = format!("{plugin_name}@{marketplace_name}");
    let marketplace_path = format!("{preview}/marketplace.json");
    let plugin_path = format!("{preview}/plugins/{plugin_name}");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let plugin_id = plugin_id.clone();
            let marketplace_path = marketplace_path.clone();
            let plugin_path = plugin_path.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                let mut plugin_installed = false;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "plugin/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "marketplaces": [{
                                            "name": marketplace_name,
                                            "path": marketplace_path.clone(),
                                            "interface": {
                                                "displayName": marketplace_name,
                                            },
                                            "plugins": [{
                                                "id": plugin_id.clone(),
                                                "name": plugin_name,
                                                "source": {
                                                    "type": "local",
                                                    "path": plugin_path.clone(),
                                                },
                                                "installed": plugin_installed,
                                                "enabled": plugin_installed,
                                                "installPolicy": "AVAILABLE",
                                                "authPolicy": "ON_INSTALL",
                                                "interface": {
                                                    "displayName": plugin_name,
                                                    "shortDescription": format!("{plugin_name} short description"),
                                                    "longDescription": null,
                                                    "developerName": null,
                                                    "category": "tools",
                                                    "capabilities": ["commands"],
                                                    "websiteUrl": null,
                                                    "privacyPolicyUrl": null,
                                                    "termsOfServiceUrl": null,
                                                    "defaultPrompt": null,
                                                    "brandColor": null,
                                                    "composerIcon": null,
                                                    "composerIconUrl": null,
                                                    "logo": null,
                                                    "logoUrl": null,
                                                    "screenshots": [],
                                                    "screenshotUrls": [],
                                                },
                                            }],
                                        }],
                                        "marketplaceLoadErrors": [],
                                        "featuredPluginIds": [plugin_id.clone()],
                                    }),
                                }),
                            )
                            .await;
                        }
                        "plugin/read" => {
                            let plugin_read_params: PluginReadParams = serde_json::from_value(
                                request
                                    .params
                                    .clone()
                                    .expect("plugin/read params should exist"),
                            )
                            .expect("plugin/read params should decode");
                            if plugin_read_params.plugin_name != plugin_name
                                || plugin_read_params
                                    .marketplace_path
                                    .as_ref()
                                    .is_none_or(|path| {
                                        path.as_path() != std::path::Path::new(&marketplace_path)
                                    })
                            {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        id: request.id,
                                        error: JSONRPCErrorError {
                                            code: -32602,
                                            message: "plugin not found on worker".to_string(),
                                            data: None,
                                        },
                                    }),
                                )
                                .await;
                                continue;
                            }
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "plugin": {
                                            "marketplaceName": marketplace_name,
                                            "marketplacePath": marketplace_path.clone(),
                                            "summary": {
                                                "id": plugin_id.clone(),
                                                "name": plugin_name,
                                                "source": {
                                                    "type": "local",
                                                    "path": plugin_path.clone(),
                                                },
                                                "installed": plugin_installed,
                                                "enabled": plugin_installed,
                                                "installPolicy": "AVAILABLE",
                                                "authPolicy": "ON_INSTALL",
                                                "interface": {
                                                    "displayName": plugin_name,
                                                    "shortDescription": format!("{plugin_name} short description"),
                                                    "longDescription": null,
                                                    "developerName": null,
                                                    "category": "tools",
                                                    "capabilities": ["commands"],
                                                    "websiteUrl": null,
                                                    "privacyPolicyUrl": null,
                                                    "termsOfServiceUrl": null,
                                                    "defaultPrompt": null,
                                                    "brandColor": null,
                                                    "composerIcon": null,
                                                    "composerIconUrl": null,
                                                    "logo": null,
                                                    "logoUrl": null,
                                                    "screenshots": [],
                                                    "screenshotUrls": [],
                                                },
                                            },
                                            "description": description,
                                            "skills": [],
                                            "hooks": [],
                                            "apps": [],
                                            "appTemplates": [],
                                            "mcpServers": [],
                                        },
                                    }),
                                }),
                            )
                            .await;
                        }
                        "plugin/install" => {
                            let plugin_install_params: PluginInstallParams =
                                serde_json::from_value(
                                    request
                                        .params
                                        .clone()
                                        .expect("plugin/install params should exist"),
                                )
                                .expect("plugin/install params should decode");
                            if plugin_install_params.plugin_name != plugin_name
                                || plugin_install_params.marketplace_path.as_ref().is_none_or(
                                    |path| {
                                        path.as_path() != std::path::Path::new(&marketplace_path)
                                    },
                                )
                            {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        id: request.id,
                                        error: JSONRPCErrorError {
                                            code: -32602,
                                            message: "plugin not found on worker".to_string(),
                                            data: None,
                                        },
                                    }),
                                )
                                .await;
                                continue;
                            }
                            plugin_installed = true;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "authPolicy": "ON_INSTALL",
                                        "appsNeedingAuth": [],
                                    }),
                                }),
                            )
                            .await;
                        }
                        "plugin/uninstall" => {
                            let plugin_uninstall_params: PluginUninstallParams =
                                serde_json::from_value(
                                    request
                                        .params
                                        .clone()
                                        .expect("plugin/uninstall params should exist"),
                                )
                                .expect("plugin/uninstall params should decode");
                            if plugin_uninstall_params.plugin_id != plugin_id {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        id: request.id,
                                        error: JSONRPCErrorError {
                                            code: -32602,
                                            message: "plugin not found on worker".to_string(),
                                            data: None,
                                        },
                                    }),
                                )
                                .await;
                                continue;
                            }
                            plugin_installed = false;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                        }
                        _ => panic!("unexpected request method: {}", request.method),
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_reconnecting_v2_multi_connection_plugin_server(
    marketplace_name: &'static str,
    preview: &'static str,
    plugin_name: &'static str,
    description: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let plugin_id = format!("{plugin_name}@{marketplace_name}");
    let marketplace_path = format!("{preview}/marketplace.json");
    let plugin_path = format!("{preview}/plugins/{plugin_name}");
    tokio::spawn(async move {
        for connection_index in 0..4 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let plugin_id = plugin_id.clone();
            let marketplace_path = marketplace_path.clone();
            let plugin_path = plugin_path.clone();
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            expect_remote_initialize(&mut websocket).await;
            match connection_index {
                0 | 2 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                3 => {
                    let mut plugin_installed = false;
                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        match request.method.as_str() {
                            "configRequirements/read" => {
                                pretty_assertions::assert_eq!(request.params, None);
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "requirements": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "marketplaces": [{
                                                "name": marketplace_name,
                                                "path": marketplace_path.clone(),
                                                "interface": {
                                                    "displayName": marketplace_name,
                                                },
                                                "plugins": [{
                                                    "id": plugin_id.clone(),
                                                    "name": plugin_name,
                                                    "source": {
                                                        "type": "local",
                                                        "path": plugin_path.clone(),
                                                    },
                                                    "installed": plugin_installed,
                                                    "enabled": plugin_installed,
                                                    "installPolicy": "AVAILABLE",
                                                    "authPolicy": "ON_INSTALL",
                                                    "interface": {
                                                        "displayName": plugin_name,
                                                        "shortDescription": format!("{plugin_name} short description"),
                                                        "longDescription": null,
                                                        "developerName": null,
                                                        "category": "tools",
                                                        "capabilities": ["commands"],
                                                        "websiteUrl": null,
                                                        "privacyPolicyUrl": null,
                                                        "termsOfServiceUrl": null,
                                                        "defaultPrompt": null,
                                                        "brandColor": null,
                                                        "composerIcon": null,
                                                        "composerIconUrl": null,
                                                        "logo": null,
                                                        "logoUrl": null,
                                                        "screenshots": [],
                                                        "screenshotUrls": [],
                                                    },
                                                }],
                                            }],
                                            "marketplaceLoadErrors": [],
                                            "featuredPluginIds": [plugin_id.clone()],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/read" => {
                                let plugin_read_params: PluginReadParams = serde_json::from_value(
                                    request
                                        .params
                                        .clone()
                                        .expect("plugin/read params should exist"),
                                )
                                .expect("plugin/read params should decode");
                                if plugin_read_params.plugin_name != plugin_name
                                    || plugin_read_params.marketplace_path.as_ref().is_none_or(
                                        |path| {
                                            path.as_path()
                                                != std::path::Path::new(&marketplace_path)
                                        },
                                    )
                                {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "plugin": {
                                                "marketplaceName": marketplace_name,
                                                "marketplacePath": marketplace_path.clone(),
                                                "summary": {
                                                    "id": plugin_id.clone(),
                                                    "name": plugin_name,
                                                    "source": {
                                                        "type": "local",
                                                        "path": plugin_path.clone(),
                                                    },
                                                    "installed": plugin_installed,
                                                    "enabled": plugin_installed,
                                                    "installPolicy": "AVAILABLE",
                                                    "authPolicy": "ON_INSTALL",
                                                    "interface": {
                                                        "displayName": plugin_name,
                                                        "shortDescription": format!("{plugin_name} short description"),
                                                        "longDescription": null,
                                                        "developerName": null,
                                                        "category": "tools",
                                                        "capabilities": ["commands"],
                                                        "websiteUrl": null,
                                                        "privacyPolicyUrl": null,
                                                        "termsOfServiceUrl": null,
                                                        "defaultPrompt": null,
                                                        "brandColor": null,
                                                        "composerIcon": null,
                                                        "composerIconUrl": null,
                                                        "logo": null,
                                                        "logoUrl": null,
                                                        "screenshots": [],
                                                        "screenshotUrls": [],
                                                    },
                                                },
                                                "description": description,
                                                "skills": [],
                                                "hooks": [],
                                                "apps": [],
                                                "appTemplates": [],
                                                "mcpServers": [],
                                            },
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/install" => {
                                let plugin_install_params: PluginInstallParams =
                                    serde_json::from_value(
                                        request
                                            .params
                                            .clone()
                                            .expect("plugin/install params should exist"),
                                    )
                                    .expect("plugin/install params should decode");
                                if plugin_install_params.plugin_name != plugin_name
                                    || plugin_install_params.marketplace_path.as_ref().is_none_or(
                                        |path| {
                                            path.as_path()
                                                != std::path::Path::new(&marketplace_path)
                                        },
                                    )
                                {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                plugin_installed = true;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "authPolicy": "ON_INSTALL",
                                            "appsNeedingAuth": [],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/uninstall" => {
                                let plugin_uninstall_params: PluginUninstallParams =
                                    serde_json::from_value(
                                        request
                                            .params
                                            .clone()
                                            .expect("plugin/uninstall params should exist"),
                                    )
                                    .expect("plugin/uninstall params should decode");
                                if plugin_uninstall_params.plugin_id != plugin_id {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                plugin_installed = false;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                            }
                            method => panic!("unexpected request method: {method}"),
                        }
                    }
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}
