use super::*;

pub(crate) async fn start_mock_remote_multi_connection_thread_server(
    thread_id: &'static str,
    preview: &'static str,
) -> String {
    let thread_id = thread_id.to_string();
    let preview = preview.to_string();
    let rollout_path = format!("{preview}/rollout.jsonl");
    let forked_thread_id = format!("{thread_id}-fork");
    let forked_preview = format!("{preview}-fork");
    let forked_rollout_path = format!("{forked_preview}/rollout.jsonl");
    let app_id = format!("{thread_id}-app");
    let app_name = format!("{thread_id} app");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let thread_id = thread_id.clone();
            let preview = preview.clone();
            let rollout_path = rollout_path.clone();
            let forked_thread_id = forked_thread_id.clone();
            let forked_preview = forked_preview.clone();
            let forked_rollout_path = forked_rollout_path.clone();
            let app_id = app_id.clone();
            let app_name = app_name.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "thread/start" => serde_json::json!({
                            "thread": mock_thread_with_path(&thread_id, &preview, Some(rollout_path.as_str())),
                            "model": "gpt-5",
                            "modelProvider": "openai",
                            "serviceTier": null,
                            "cwd": preview.as_str(),
                            "instructionSources": [],
                            "approvalPolicy": "never",
                            "approvalsReviewer": "user",
                            "sandbox": {
                                "type": "dangerFullAccess"
                            },
                            "reasoningEffort": null,
                        }),
                        "thread/list" => serde_json::json!({
                            "data": [mock_thread_with_path(&thread_id, &preview, Some(rollout_path.as_str()))],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        "thread/loaded/list" => serde_json::json!({
                            "data": [thread_id.as_str()],
                            "nextCursor": null,
                        }),
                        "thread/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            assert!(
                                requested_thread_id == thread_id
                                    || requested_thread_id == forked_thread_id,
                                "thread/read should stay on the owning worker"
                            );
                            let (response_thread_id, response_preview) =
                                if requested_thread_id == thread_id {
                                    (&thread_id, &preview)
                                } else {
                                    (&forked_thread_id, &forked_preview)
                                };
                            let response_rollout_path = if requested_thread_id == thread_id {
                                rollout_path.as_str()
                            } else {
                                forked_rollout_path.as_str()
                            };
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    response_thread_id,
                                    response_preview,
                                    Some(response_rollout_path),
                                ),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": response_preview.as_str(),
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/resume" => {
                            let history_resume = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("history"))
                                .is_some_and(|history| !history.is_null());
                            let path_resume = request
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
                            if let Some(requested_path) = path_resume {
                                pretty_assertions::assert_eq!(
                                    requested_path,
                                    rollout_path,
                                    "path-based thread/resume should stay on the owning worker"
                                );
                            } else if !history_resume {
                                pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            }
                            serde_json::json!({
                                "thread": mock_thread_with_path(&thread_id, &preview, Some(rollout_path.as_str())),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview.as_str(),
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
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
                                pretty_assertions::assert_eq!(
                                    requested_path,
                                    rollout_path,
                                    "path-based thread/fork should stay on the owning worker"
                                );
                            } else {
                                pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            }
                            serde_json::json!({
                                "thread": {
                                    "id": forked_thread_id.as_str(),
                                    "sessionId": forked_thread_id.as_str(),
                                    "forkedFromId": thread_id.as_str(),
                                    "preview": forked_preview.as_str(),
                                    "ephemeral": true,
                                    "modelProvider": "openai",
                                    "createdAt": 1,
                                    "updatedAt": if forked_thread_id.ends_with("-b") { 2 } else { 1 },
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
                                "cwd": forked_preview.as_str(),
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "app/list" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("app/list should include threadId on thread worker");
                            assert!(
                                requested_thread_id == thread_id
                                    || requested_thread_id == forked_thread_id,
                                "thread-scoped app/list should stay on the owning worker"
                            );
                            serde_json::json!({
                                "data": [{
                                    "id": app_id,
                                    "name": app_name,
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
                            })
                        }
                        "mcpServer/resource/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/resource/read should include threadId");
                            assert!(
                                requested_thread_id == thread_id
                                    || requested_thread_id == forked_thread_id,
                                "thread-scoped MCP resource reads should stay on the owning worker"
                            );
                            let uri = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("uri"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/resource/read should include uri");
                            serde_json::json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "text/markdown",
                                    "text": format!("{thread_id} resource"),
                                }],
                            })
                        }
                        "mcpServer/tool/call" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/tool/call should include threadId");
                            assert!(
                                requested_thread_id == thread_id
                                    || requested_thread_id == forked_thread_id,
                                "thread-scoped MCP tool calls should stay on the owning worker"
                            );
                            let tool = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("tool"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/tool/call should include tool");
                            pretty_assertions::assert_eq!(tool, "lookup");
                            serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!("{thread_id} tool result"),
                                }],
                                "structuredContent": {
                                    "threadId": thread_id,
                                },
                                "isError": false,
                            })
                        }
                        _ => panic!("unexpected request method: {}", request.method),
                    };

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_hidden_server_request_server()
-> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (rejection_tx, rejection_rx) = oneshot::channel::<String>();
    let rejection_tx = Arc::new(Mutex::new(Some(rejection_tx)));
    let thread_started = Arc::new(Mutex::new(false));
    let hidden_request_sent = Arc::new(Mutex::new(false));
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let rejection_tx = rejection_tx.clone();
            let thread_started = thread_started.clone();
            let hidden_request_sent = hidden_request_sent.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                let should_send_hidden_request = {
                    let thread_started = *thread_started.lock().await;
                    let mut hidden_request_sent = hidden_request_sent.lock().await;
                    if thread_started && !*hidden_request_sent {
                        *hidden_request_sent = true;
                        true
                    } else {
                        false
                    }
                };

                if should_send_hidden_request {
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("hidden-server-request".to_string()),
                            method: "item/commandExecution/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": "thread-worker-a",
                                "turnId": "turn-hidden",
                                "itemId": "item-hidden",
                                "startedAtMs": 0,
                                "reason": "Need to run a hidden command",
                                "command": "pwd",
                            })),
                            trace: None,
                        }),
                    )
                    .await;

                    let error = loop {
                        match read_websocket_message(&mut websocket).await {
                            JSONRPCMessage::Error(error) => break error,
                            JSONRPCMessage::Notification(notification)
                                if notification.method == "initialized" =>
                            {
                                continue;
                            }
                            other => {
                                panic!("expected hidden server request rejection, got {other:?}");
                            }
                        }
                    };
                    pretty_assertions::assert_eq!(
                        error.id,
                        RequestId::String("hidden-server-request".to_string())
                    );
                    pretty_assertions::assert_eq!(error.error.code, -32602);
                    pretty_assertions::assert_eq!(error.error.message, "thread not found");
                    if let Some(rejection_tx) = rejection_tx.lock().await.take() {
                        let _ = rejection_tx.send(error.error.message.clone());
                    }
                }

                while let Some(frame) = websocket.next().await {
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };
                    let result = match request.method.as_str() {
                        "thread/start" => {
                            *thread_started.lock().await = true;
                            serde_json::json!({
                                "thread": mock_thread("thread-worker-a", "/tmp/worker-a"),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/worker-a",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/list" => serde_json::json!({
                            "data": [mock_thread("thread-worker-a", "/tmp/worker-a")],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        other => panic!("unexpected hidden server request method: {other}"),
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    (format!("ws://{addr}"), rejection_rx)
}
