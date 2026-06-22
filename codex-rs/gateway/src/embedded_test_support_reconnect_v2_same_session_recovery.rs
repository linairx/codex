use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_thread_server_for_same_session_recovery(
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
                    0 | 2 => {
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
                        let mut thread_name = None::<String>;
                        let review_thread_id = format!("{thread_id}-review");
                        let review_turn_id = format!("turn-review-{thread_id}");
                        let turn_id = format!("turn-{thread_id}");
                        let rollout_path = format!("{preview}/rollout.jsonl");
                        let forked_thread_id = format!("{thread_id}-fork");
                        let forked_preview = format!("{preview}-fork");
                        let forked_rollout_path = format!("{forked_preview}/rollout.jsonl");
                        loop {
                            let request = read_websocket_request(&mut websocket).await;
                            match request.method.as_str() {
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
                                "thread/start" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": mock_thread_with_name_and_path(
                                                    thread_id,
                                                    preview,
                                                    thread_name.as_deref(),
                                                    Some(rollout_path.as_str()),
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
                                            || requested_thread_id == review_thread_id
                                            || requested_thread_id == forked_thread_id,
                                        "thread/read should target the recovered thread, its fork, or its review thread",
                                    );
                                    let thread = if requested_thread_id == thread_id {
                                        mock_thread_with_name_and_path(
                                            thread_id,
                                            preview,
                                            thread_name.as_deref(),
                                            Some(rollout_path.as_str()),
                                        )
                                    } else if requested_thread_id == forked_thread_id {
                                        mock_thread_with_path(
                                            &forked_thread_id,
                                            &forked_preview,
                                            Some(forked_rollout_path.as_str()),
                                        )
                                    } else {
                                        mock_thread_with_path(
                                            &review_thread_id,
                                            preview,
                                            Some(rollout_path.as_str()),
                                        )
                                    };
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": thread,
                                                "turns": [],
                                            }),
                                        }),
                                    )
                                    .await;
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
                                            requested_path, rollout_path,
                                            "path-based thread/resume should stay on the recovered worker",
                                        );
                                    } else {
                                        assert_eq!(requested_thread_id, thread_id);
                                    }
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": mock_thread_with_name_and_path(
                                                    thread_id,
                                                    preview,
                                                    thread_name.as_deref(),
                                                    Some(rollout_path.as_str()),
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
                                            requested_path, rollout_path,
                                            "path-based thread/fork should stay on the recovered worker",
                                        );
                                    } else {
                                        assert_eq!(requested_thread_id, thread_id);
                                    }
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": {
                                                    "id": forked_thread_id.as_str(),
                                                    "sessionId": forked_thread_id.as_str(),
                                                    "forkedFromId": thread_id,
                                                    "preview": forked_preview.as_str(),
                                                    "ephemeral": true,
                                                    "modelProvider": "openai",
                                                    "createdAt": 1,
                                                    "updatedAt": 1,
                                                    "status": {
                                                        "type": "idle"
                                                    },
                                                    "path": forked_rollout_path.as_str(),
                                                    "cwd": forked_preview.as_str(),
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
                                                "cwd": forked_preview,
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
                                    thread_name = Some(name.to_string());
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
                                                method: "thread/name/updated".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "threadName": name,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
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
                                    assert_eq!(requested_thread_id, thread_id);
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
                                    assert_eq!(requested_thread_id, thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                    assert_eq!(requested_thread_id, thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "data": [mock_turn(&turn_id, "completed")],
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
                                        .expect(
                                            "thread/increment_elicitation should include threadId",
                                        );
                                    assert_eq!(requested_thread_id, thread_id);
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
                                        .expect(
                                            "thread/decrement_elicitation should include threadId",
                                        );
                                    assert_eq!(requested_thread_id, thread_id);
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
                                    assert_eq!(requested_thread_id, thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                }
                                "thread/backgroundTerminals/clean" => {
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect(
                                            "thread/backgroundTerminals/clean should include threadId",
                                        );
                                    assert_eq!(requested_thread_id, thread_id);
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
                                    assert_eq!(requested_thread_id, thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
                                }
                                "turn/start" => {
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect("turn/start should include threadId");
                                    assert_eq!(requested_thread_id, thread_id);
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
                                                method: "item/reasoning/summaryPartAdded"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "turnId": turn_id,
                                                    "itemId": format!("reasoning-summary-{thread_id}"),
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "turnId": turn_id,
                                            }),
                                        }),
                                    )
                                    .await;
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
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                }
                                "thread/realtime/start" => {
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect("thread/realtime/start should include threadId");
                                    assert_eq!(requested_thread_id, thread_id);
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
                                                    "realtimeSessionId": format!("session-{thread_id}"),
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
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect(
                                            "thread/realtime/appendText should include threadId",
                                        );
                                    assert_eq!(requested_thread_id, thread_id);
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
                                                method: "thread/realtime/transcript/delta"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "role": "assistant",
                                                    "delta": format!("delta {thread_id}"),
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/transcript/done"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "role": "assistant",
                                                    "text": format!("done {thread_id}"),
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/realtime/appendAudio" => {
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect(
                                            "thread/realtime/appendAudio should include threadId",
                                        );
                                    assert_eq!(requested_thread_id, thread_id);
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
                                    let requested_thread_id = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("threadId"))
                                        .and_then(serde_json::Value::as_str)
                                        .expect("thread/realtime/stop should include threadId");
                                    assert_eq!(requested_thread_id, thread_id);
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
                                "thread/list" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
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
