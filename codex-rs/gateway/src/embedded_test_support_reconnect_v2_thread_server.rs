use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_thread_server(
    thread_id: &'static str,
    preview: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let mut connection_index = 0usize;
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let current_connection_index = connection_index;
            connection_index = connection_index.saturating_add(1);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match current_connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    _ => {
                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            tracing::info!(
                                method = %request.method,
                                "realtime reconnect helper received request",
                            );
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
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Request(
                                            codex_app_server_protocol::JSONRPCRequest {
                                                id: RequestId::String(
                                                    "srv-user-input-after-reconnect"
                                                        .to_string(),
                                                ),
                                                method: "item/tool/requestUserInput".to_string(),
                                                params: Some(
                                                    serde_json::to_value(
                                                        codex_app_server_protocol::ToolRequestUserInputParams {
                                                            auto_resolution_ms: None,
                                                            thread_id: thread_id.to_string(),
                                                            turn_id: "turn-worker-a-2".to_string(),
                                                            item_id: "tool-call-worker-a-2".to_string(),
                                                            questions: vec![ToolRequestUserInputQuestion {
                                                                id: "mode".to_string(),
                                                                header: "Mode".to_string(),
                                                                question: "Pick execution mode".to_string(),
                                                                is_other: false,
                                                                is_secret: false,
                                                                options: Some(vec![]),
                                                            }],
                                                        },
                                                    )
                                                    .expect("server request params should serialize"),
                                                ),
                                                trace: None,
                                            },
                                        ),
                                    )
                                    .await;
                                    let JSONRPCMessage::Response(response) =
                                        read_websocket_message(&mut websocket).await
                                    else {
                                        panic!("expected server request response");
                                    };
                                    assert_eq!(
                                        response.id,
                                        RequestId::String(
                                            "srv-user-input-after-reconnect".to_string()
                                        )
                                    );
                                    assert_eq!(
                                        response.result,
                                        serde_json::json!({
                                            "answers": {
                                                "mode": {
                                                    "answers": ["safe"],
                                                },
                                            },
                                        })
                                    );
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "serverRequest/resolved".to_string(),
                                                params: Some(serde_json::to_value(
                                                    ServerRequestResolvedNotification {
                                                        thread_id: thread_id.to_string(),
                                                        request_id: RequestId::String(
                                                            "srv-user-input-after-reconnect"
                                                                .to_string(),
                                                        ),
                                                    },
                                                )
                                                .expect(
                                                    "user input serverRequest/resolved should serialize",
                                                )),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/read" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": mock_thread(thread_id, preview),
                                                "turns": [],
                                            }),
                                        }),
                                    )
                                    .await;
                                }
                                "thread/list" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "data": [mock_thread(thread_id, preview)],
                                                "nextCursor": null,
                                                "backwardsCursor": null,
                                            }),
                                        }),
                                    )
                                    .await;
                                }
                                "thread/loaded/list" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "data": [thread_id],
                                                "nextCursor": null,
                                            }),
                                        }),
                                    )
                                    .await;
                                }
                                "configRequirements/read" => {
                                    assert_eq!(request.params, None);
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
                                "turn/start" => {
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect("turn/start should include threadId");
                                    assert_eq!(requested_thread_id, thread_id);
                                    let turn_id = format!("turn-{thread_id}");

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
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "hook/started".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "turnId": turn_id,
                                                    "run": {
                                                        "id": "hook-remote-workflow",
                                                        "eventName": "userPromptSubmit",
                                                        "handlerType": "command",
                                                        "executionMode": "sync",
                                                        "scope": "turn",
                                                        "sourcePath": "/tmp/remote-hooks.json",
                                                        "source": "user",
                                                        "displayOrder": 0,
                                                        "status": "running",
                                                        "statusMessage": "remote hook started",
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
                                                        "id": "msg-remote-workflow",
                                                        "text": "streaming answer in progress",
                                                        "phase": "commentary",
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
                                                        "phase": "commentary",
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
                                                    "delta": "hello from recovered worker",
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
                                                    "itemId": format!("reasoning-summary-{thread_id}"),
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
                                                method: "item/commandExecution/outputDelta"
                                                    .to_string(),
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
                                                method: "item/autoApprovalReview/started"
                                                    .to_string(),
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
                                                method: "item/autoApprovalReview/completed"
                                                    .to_string(),
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
                                                    "turn": mock_turn(&turn_id, "completed"),
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                method => panic!("unexpected request method: {method}"),
                            }
                        }
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}
