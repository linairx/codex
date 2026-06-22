use super::*;

pub(crate) async fn start_mock_remote_multi_connection_concurrent_server_request_server(
    thread_id: &'static str,
    preview: &'static str,
    expected_answer: &'static str,
    request_barrier: Arc<Barrier>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let request_barrier = request_barrier.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                let thread_start_request = read_websocket_request(&mut websocket).await;
                pretty_assertions::assert_eq!(thread_start_request.method, "thread/start");
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Response(JSONRPCResponse {
                        id: thread_start_request.id,
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

                request_barrier.wait().await;

                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                        id: RequestId::String("shared-concurrent-server-request".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        params: Some(
                            serde_json::to_value(
                                codex_app_server_protocol::ToolRequestUserInputParams {
                                    auto_resolution_ms: None,
                                    thread_id: thread_id.to_string(),
                                    turn_id: "turn-remote-workflow".to_string(),
                                    item_id: "tool-call-remote-workflow".to_string(),
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

                let JSONRPCMessage::Response(response) =
                    read_websocket_message(&mut websocket).await
                else {
                    panic!("expected server request response");
                };
                pretty_assertions::assert_eq!(
                    response.id,
                    RequestId::String("shared-concurrent-server-request".to_string())
                );
                pretty_assertions::assert_eq!(
                    response.result,
                    serde_json::json!({
                        "answers": {
                            "mode": {
                                "answers": [expected_answer],
                            },
                        },
                    })
                );

                request_barrier.wait().await;

                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                        id: RequestId::String("shared-concurrent-permissions-request".to_string()),
                        method: "item/permissions/requestApproval".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": thread_id,
                            "turnId": "turn-remote-workflow",
                            "itemId": "perm-remote-workflow",
                            "startedAtMs": 0,
                            "cwd": preview,
                            "reason": "Need concurrent permissions",
                            "permissions": {
                                "fileSystem": null,
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
                    panic!("expected permissions approval response");
                };
                pretty_assertions::assert_eq!(
                    permissions_response.id,
                    RequestId::String("shared-concurrent-permissions-request".to_string())
                );
                pretty_assertions::assert_eq!(
                    permissions_response.result,
                    serde_json::json!({
                        "permissions": {
                            "network": {
                                "enabled": true,
                            },
                        },
                        "scope": "turn",
                    })
                );

                request_barrier.wait().await;

                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                        id: RequestId::String("shared-concurrent-mcp-request".to_string()),
                        method: "mcpServer/elicitation/request".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": thread_id,
                            "turnId": "turn-remote-workflow",
                            "serverName": "mock-mcp",
                            "mode": "form",
                            "_meta": null,
                            "message": "Allow concurrent action?",
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
                    panic!("expected mcp elicitation response");
                };
                pretty_assertions::assert_eq!(
                    mcp_response.id,
                    RequestId::String("shared-concurrent-mcp-request".to_string())
                );
                pretty_assertions::assert_eq!(
                    mcp_response.result,
                    serde_json::json!({
                        "action": "accept",
                        "content": {
                            "confirmed": true,
                        },
                        "_meta": null,
                    })
                );

                request_barrier.wait().await;

                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                        id: RequestId::String(
                            "shared-concurrent-chatgpt-refresh-request".to_string(),
                        ),
                        method: "account/chatgptAuthTokens/refresh".to_string(),
                        params: Some(serde_json::json!({
                            "reason": "unauthorized",
                            "previousAccountId": format!("account-{thread_id}"),
                        })),
                        trace: None,
                    }),
                )
                .await;

                let JSONRPCMessage::Response(refresh_response) =
                    read_websocket_message(&mut websocket).await
                else {
                    panic!("expected ChatGPT token-refresh response");
                };
                pretty_assertions::assert_eq!(
                    refresh_response.id,
                    RequestId::String("shared-concurrent-chatgpt-refresh-request".to_string())
                );
                pretty_assertions::assert_eq!(
                    refresh_response.result,
                    serde_json::json!({
                        "accessToken": format!("access-token-account-{thread_id}"),
                        "chatgptAccountId": format!("account-{thread_id}"),
                        "chatgptPlanType": "pro",
                    })
                );

                while let Some(frame) = websocket.next().await {
                    match frame.expect("follow-up frame should decode") {
                        Message::Close(_) => break,
                        Message::Ping(payload) => {
                            websocket
                                .send(Message::Pong(payload))
                                .await
                                .expect("pong should send");
                        }
                        Message::Text(_)
                        | Message::Binary(_)
                        | Message::Pong(_)
                        | Message::Frame(_) => {}
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}
