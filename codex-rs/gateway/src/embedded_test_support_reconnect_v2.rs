use super::*;

#[path = "embedded_test_support_reconnect_v2_thread_server.rs"]
mod embedded_test_support_reconnect_v2_thread_server;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_thread_server::*;

pub(crate) async fn start_reconnecting_v2_mock_remote_server(
    expected_auth_token: Option<String>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..8 {
            tracing::info!(connection_index, "reconnect mock waiting for connection");
            let expected_auth_token = expected_auth_token.clone();
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = accept_hdr_async(
                stream,
                move |request: &WebSocketRequest, response: WebSocketResponse| {
                    let provided_auth_token = request
                        .headers()
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    let expected_header = expected_auth_token
                        .as_ref()
                        .map(|token| format!("Bearer {token}"));
                    assert_eq!(provided_auth_token, expected_header);
                    Ok(response)
                },
            )
            .await
            .expect("websocket upgrade should succeed");

            tracing::info!(connection_index, "reconnect mock websocket accepted");
            expect_remote_initialize(&mut websocket).await;
            tracing::info!(connection_index, "reconnect mock initialize completed");
            match connection_index {
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
                2 | 3 => {
                    tokio::spawn(async move {
                        loop {
                            let frame = match websocket.next().await {
                                Some(Ok(frame)) => frame,
                                Some(Err(error)) => {
                                    panic!("websocket frame should decode: {error}")
                                }
                                None => panic!("websocket should stay open"),
                            };
                            let request = match frame {
                                Message::Text(text) => {
                                    serde_json::from_str::<JSONRPCMessage>(&text)
                                        .expect("text frame should be valid JSON-RPC")
                                }
                                Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {
                                    continue;
                                }
                                Message::Ping(payload) => {
                                    websocket
                                        .send(Message::Pong(payload))
                                        .await
                                        .expect("pong should send");
                                    continue;
                                }
                                Message::Close(_) => return,
                            };
                            let request = match request {
                                JSONRPCMessage::Request(request) => request,
                                JSONRPCMessage::Notification(notification)
                                    if notification.method == "initialized" =>
                                {
                                    continue;
                                }
                                other => panic!("expected request, got {other:?}"),
                            };
                            tracing::info!(
                                connection_index,
                                method = %request.method,
                                "reconnect mock worker received request",
                            );
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
                                                "thread": mock_thread(
                                                    "thread-worker-a-2",
                                                    "/tmp/worker-a-2",
                                                ),
                                                "model": "gpt-5",
                                                "modelProvider": "openai",
                                                "serviceTier": null,
                                                "cwd": "/tmp/worker-a-2",
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
                                    break;
                                }
                                method => panic!("unexpected request method: {method}"),
                            }
                        }
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String("srv-user-input-after-reconnect".to_string()),
                                method: "item/tool/requestUserInput".to_string(),
                                params: Some(
                                    serde_json::to_value(
                                        codex_app_server_protocol::ToolRequestUserInputParams {
                                            auto_resolution_ms: None,
                                            thread_id: "thread-worker-a-2".to_string(),
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
                            }),
                        )
                        .await;
                        tracing::info!(connection_index, "reconnect mock sent user-input request");

                        let JSONRPCMessage::Response(response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected server request response");
                        };
                        tracing::info!(
                            connection_index,
                            response_id = ?response.id,
                            "reconnect mock received user-input response"
                        );
                        assert_eq!(
                            response.id,
                            RequestId::String("srv-user-input-after-reconnect".to_string())
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
                                    params: Some(
                                        serde_json::to_value(ServerRequestResolvedNotification {
                                            thread_id: "thread-worker-a-2".to_string(),
                                            request_id: RequestId::String(
                                                "srv-user-input-after-reconnect".to_string(),
                                            ),
                                        })
                                        .expect(
                                            "user input serverRequest/resolved should serialize",
                                        ),
                                    ),
                                },
                            ),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String("srv-command-after-reconnect".to_string()),
                                method: "item/commandExecution/requestApproval".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-worker-a-2",
                                    "turnId": "turn-worker-a-2",
                                    "itemId": "cmd-worker-a-2",
                                    "startedAtMs": 0,
                                    "command": "pwd",
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(command_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected command approval response after reconnect");
                        };
                        assert_eq!(
                            command_response.id,
                            RequestId::String("srv-command-after-reconnect".to_string())
                        );
                        assert_eq!(
                            command_response.result,
                            serde_json::json!({
                                "decision": "accept",
                            })
                        );

                        write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "serverRequest/resolved".to_string(),
                                        params: Some(
                                            serde_json::to_value(ServerRequestResolvedNotification {
                                                thread_id: "thread-worker-a-2".to_string(),
                                                request_id: RequestId::String(
                                                    "srv-command-after-reconnect".to_string(),
                                                ),
                                            })
                                            .expect(
                                                "command approval serverRequest/resolved should serialize",
                                            ),
                                        ),
                                    },
                                ),
                            )
                            .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String("srv-file-after-reconnect".to_string()),
                                method: "item/fileChange/requestApproval".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-worker-a-2",
                                    "turnId": "turn-worker-a-2",
                                    "itemId": "file-worker-a-2",
                                    "startedAtMs": 0,
                                    "reason": "Need reconnect write",
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(file_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected file approval response after reconnect");
                        };
                        assert_eq!(
                            file_response.id,
                            RequestId::String("srv-file-after-reconnect".to_string())
                        );
                        assert_eq!(
                            file_response.result,
                            serde_json::json!({
                                "decision": "accept",
                            })
                        );

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "serverRequest/resolved".to_string(),
                                    params: Some(
                                        serde_json::to_value(ServerRequestResolvedNotification {
                                            thread_id: "thread-worker-a-2".to_string(),
                                            request_id: RequestId::String(
                                                "srv-file-after-reconnect".to_string(),
                                            ),
                                        })
                                        .expect(
                                            "file approval serverRequest/resolved should serialize",
                                        ),
                                    ),
                                },
                            ),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String(
                                    "srv-mcp-elicitation-after-reconnect".to_string(),
                                ),
                                method: "mcpServer/elicitation/request".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-worker-a-2",
                                    "turnId": "turn-worker-a-2",
                                    "serverName": "mock-mcp",
                                    "mode": "form",
                                    "_meta": null,
                                    "message": "Allow reconnect action?",
                                    "requestedSchema": {
                                        "type": "object",
                                        "properties": {
                                            "confirmed": {
                                                "type": "boolean",
                                            },
                                        },
                                        "required": ["confirmed"],
                                    },
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(mcp_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected mcp elicitation response after reconnect");
                        };
                        assert_eq!(
                            mcp_response.id,
                            RequestId::String("srv-mcp-elicitation-after-reconnect".to_string())
                        );
                        assert_eq!(
                            mcp_response.result,
                            serde_json::json!({
                                "action": "accept",
                                "content": {
                                    "confirmed": true,
                                },
                                "_meta": null,
                            })
                        );

                        write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "serverRequest/resolved".to_string(),
                                        params: Some(
                                            serde_json::to_value(ServerRequestResolvedNotification {
                                                thread_id: "thread-worker-a-2".to_string(),
                                                request_id: RequestId::String(
                                                    "srv-mcp-elicitation-after-reconnect"
                                                        .to_string(),
                                                ),
                                            })
                                            .expect(
                                                "mcp elicitation serverRequest/resolved should serialize",
                                            ),
                                        ),
                                    },
                                ),
                            )
                            .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "item/started".to_string(),
                                    params: Some(
                                        serde_json::to_value(ItemStartedNotification {
                                            started_at_ms: 0,
                                            thread_id: "thread-worker-a-2".to_string(),
                                            turn_id: "turn-worker-a-2".to_string(),
                                            item: ThreadItem::DynamicToolCall {
                                                namespace: None,
                                                id: "tool-call-after-reconnect".to_string(),
                                                tool: "image-edit".to_string(),
                                                arguments: serde_json::json!({
                                                    "prompt": "Sharpen image after reconnect",
                                                    "strength": 0.75,
                                                }),
                                                status: DynamicToolCallStatus::InProgress,
                                                content_items: None,
                                                success: None,
                                                duration_ms: None,
                                            },
                                        })
                                        .expect("dynamic tool item/started should serialize"),
                                    ),
                                },
                            ),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String(
                                    "srv-dynamic-tool-after-reconnect".to_string(),
                                ),
                                method: "item/tool/call".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-worker-a-2",
                                    "turnId": "turn-worker-a-2",
                                    "callId": "tool-call-after-reconnect",
                                    "tool": "image-edit",
                                    "arguments": {
                                        "prompt": "Sharpen image after reconnect",
                                        "strength": 0.75,
                                    },
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(dynamic_tool_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected dynamic tool response after reconnect");
                        };
                        assert_eq!(
                            dynamic_tool_response.id,
                            RequestId::String("srv-dynamic-tool-after-reconnect".to_string())
                        );
                        assert_eq!(
                            dynamic_tool_response.result,
                            serde_json::json!({
                                "contentItems": [{
                                    "type": "inputText",
                                    "text": "tool output after reconnect",
                                }],
                                "success": true,
                            })
                        );

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "serverRequest/resolved".to_string(),
                                    params: Some(
                                        serde_json::to_value(ServerRequestResolvedNotification {
                                            thread_id: "thread-worker-a-2".to_string(),
                                            request_id: RequestId::String(
                                                "srv-dynamic-tool-after-reconnect".to_string(),
                                            ),
                                        })
                                        .expect(
                                            "dynamic tool serverRequest/resolved should serialize",
                                        ),
                                    ),
                                },
                            ),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String(
                                    "srv-permissions-after-reconnect".to_string(),
                                ),
                                method: "item/permissions/requestApproval".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-worker-a-2",
                                    "turnId": "turn-worker-a-2",
                                    "itemId": "perm-worker-a-2",
                                    "startedAtMs": 0,
                                    "cwd": "/tmp/worker-a-2",
                                    "reason": "Need reconnect permissions",
                                    "permissions": {
                                        "network": {
                                            "enabled": true,
                                        },
                                    },
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(permissions_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected permissions approval response after reconnect");
                        };
                        assert_eq!(
                            permissions_response.id,
                            RequestId::String("srv-permissions-after-reconnect".to_string())
                        );
                        assert_eq!(
                            permissions_response.result,
                            serde_json::json!({
                                "permissions": {
                                    "fileSystem": null,
                                    "network": {
                                        "enabled": true,
                                    },
                                },
                                "scope": "turn",
                            })
                        );

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "serverRequest/resolved".to_string(),
                                    params: Some(
                                        serde_json::to_value(ServerRequestResolvedNotification {
                                            thread_id: "thread-worker-a-2".to_string(),
                                            request_id: RequestId::String(
                                                "srv-permissions-after-reconnect".to_string(),
                                            ),
                                        })
                                        .expect(
                                            "permissions serverRequest/resolved should serialize",
                                        ),
                                    ),
                                },
                            ),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String(
                                    "srv-chatgpt-refresh-after-reconnect".to_string(),
                                ),
                                method: "account/chatgptAuthTokens/refresh".to_string(),
                                params: Some(serde_json::json!({
                                    "reason": "unauthorized",
                                    "previousAccountId": "acct-reconnect",
                                })),
                                trace: None,
                            }),
                        )
                        .await;
                        let JSONRPCMessage::Response(refresh_response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected chatgpt refresh response after reconnect");
                        };
                        assert_eq!(
                            refresh_response.id,
                            RequestId::String("srv-chatgpt-refresh-after-reconnect".to_string())
                        );
                        assert_eq!(
                            refresh_response.result,
                            serde_json::json!({
                                "accessToken": "access-token-reconnect",
                                "chatgptAccountId": "acct-reconnect",
                                "chatgptPlanType": "pro",
                            })
                        );

                        write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "serverRequest/resolved".to_string(),
                                        params: Some(
                                            serde_json::to_value(ServerRequestResolvedNotification {
                                                thread_id: "thread-worker-a-2".to_string(),
                                                request_id: RequestId::String(
                                                    "srv-chatgpt-refresh-after-reconnect"
                                                        .to_string(),
                                                ),
                                            })
                                            .expect(
                                                "chatgpt refresh serverRequest/resolved should serialize",
                                            ),
                                        ),
                                    },
                                ),
                            )
                            .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "item/completed".to_string(),
                                    params: Some(
                                        serde_json::to_value(ItemCompletedNotification {
                                            completed_at_ms: 0,
                                            thread_id: "thread-worker-a-2".to_string(),
                                            turn_id: "turn-worker-a-2".to_string(),
                                            item: ThreadItem::DynamicToolCall {
                                                namespace: None,
                                                id: "tool-call-after-reconnect".to_string(),
                                                tool: "image-edit".to_string(),
                                                arguments: serde_json::json!({
                                                    "prompt": "Sharpen image after reconnect",
                                                    "strength": 0.75,
                                                }),
                                                status: DynamicToolCallStatus::Completed,
                                                content_items: Some(vec![
                                                    DynamicToolCallOutputContentItem::InputText {
                                                        text: "tool output after reconnect"
                                                            .to_string(),
                                                    },
                                                ]),
                                                success: Some(true),
                                                duration_ms: Some(9),
                                            },
                                        })
                                        .expect("dynamic tool item/completed should serialize"),
                                    ),
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
                                        "threadId": "thread-worker-a-2",
                                        "turn": mock_turn("turn-worker-a-2", "completed"),
                                    })),
                                },
                            ),
                        )
                        .await;
                    });
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

#[path = "embedded_test_support_reconnect_v2_realtime.rs"]
mod embedded_test_support_reconnect_v2_realtime;
#[path = "embedded_test_support_reconnect_v2_refresh.rs"]
mod embedded_test_support_reconnect_v2_refresh;
#[path = "embedded_test_support_reconnect_v2_same_session_recovery.rs"]
mod embedded_test_support_reconnect_v2_same_session_recovery;
#[path = "embedded_test_support_reconnect_v2_session_mutation.rs"]
mod embedded_test_support_reconnect_v2_session_mutation;
#[path = "embedded_test_support_reconnect_v2_skills.rs"]
mod embedded_test_support_reconnect_v2_skills;
#[path = "embedded_test_support_reconnect_v2_state_notification.rs"]
mod embedded_test_support_reconnect_v2_state_notification;
#[path = "embedded_test_support_reconnect_v2_thread_control.rs"]
mod embedded_test_support_reconnect_v2_thread_control;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_realtime::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_refresh::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_same_session_recovery::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_session_mutation::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_skills::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_state_notification::*;
#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_reconnect_v2_thread_control::*;
