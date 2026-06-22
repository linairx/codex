use super::*;

pub(crate) async fn drive_mock_remote_multi_connection_server_request_session(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    thread_id: &str,
    preview: &str,
    expected_answer: &str,
    previous_account_id: &str,
    legacy_approval_exercise: LegacyApprovalExercise,
) {
    let thread_start_request = loop {
        let request = read_websocket_request(websocket).await;
        match request.method.as_str() {
            "configRequirements/read" => {
                pretty_assertions::assert_eq!(request.params, None);
                write_websocket_message(
                    websocket,
                    JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({
                            "requirements": null,
                        }),
                    }),
                )
                .await;
            }
            "thread/start" => break request,
            method => panic!("unexpected request method: {method}"),
        }
    };
    pretty_assertions::assert_eq!(thread_start_request.method, "thread/start");
    write_websocket_message(
        websocket,
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

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-server-request".to_string()),
            method: "item/tool/requestUserInput".to_string(),
            params: Some(
                serde_json::to_value(codex_app_server_protocol::ToolRequestUserInputParams {
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
                })
                .expect("server request params should serialize"),
            ),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Response(response) = read_websocket_message(websocket).await else {
        panic!("expected server request response");
    };
    pretty_assertions::assert_eq!(
        response.id,
        RequestId::String("shared-server-request".to_string())
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

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-command-request".to_string()),
            method: "item/commandExecution/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": "turn-remote-workflow",
                "itemId": "cmd-remote-workflow",
                "startedAtMs": 0,
                "cwd": preview,
                "command": "pwd",
            })),
            trace: None,
        }),
    )
    .await;
    let JSONRPCMessage::Response(command_response) = read_websocket_message(websocket).await else {
        panic!("expected command approval response");
    };
    pretty_assertions::assert_eq!(
        command_response.id,
        RequestId::String("shared-command-request".to_string())
    );
    pretty_assertions::assert_eq!(
        command_response.result,
        serde_json::json!({
            "decision": "accept",
        })
    );

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-file-request".to_string()),
            method: "item/fileChange/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": "turn-remote-workflow",
                "itemId": "file-remote-workflow",
                "startedAtMs": 0,
                "reason": "Need to write changes",
            })),
            trace: None,
        }),
    )
    .await;
    let JSONRPCMessage::Response(file_response) = read_websocket_message(websocket).await else {
        panic!("expected file approval response");
    };
    pretty_assertions::assert_eq!(
        file_response.id,
        RequestId::String("shared-file-request".to_string())
    );
    pretty_assertions::assert_eq!(
        file_response.result,
        serde_json::json!({
            "decision": "accept",
        })
    );

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-mcp-request".to_string()),
            method: "mcpServer/elicitation/request".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": "turn-remote-workflow",
                "serverName": "mock-mcp",
                "mode": "form",
                "_meta": null,
                "message": "Allow mock action?",
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
    let JSONRPCMessage::Response(mcp_response) = read_websocket_message(websocket).await else {
        panic!("expected mcp elicitation response");
    };
    pretty_assertions::assert_eq!(
        mcp_response.id,
        RequestId::String("shared-mcp-request".to_string())
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

    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/started".to_string(),
            params: Some(
                serde_json::to_value(ItemStartedNotification {
                    started_at_ms: 0,
                    thread_id: thread_id.to_string(),
                    turn_id: "turn-remote-workflow".to_string(),
                    item: ThreadItem::DynamicToolCall {
                        namespace: None,
                        id: "tool-call-remote-workflow".to_string(),
                        tool: "image-edit".to_string(),
                        arguments: serde_json::json!({
                            "prompt": format!("Sharpen image for {thread_id}"),
                            "strength": 0.5,
                        }),
                        status: DynamicToolCallStatus::InProgress,
                        content_items: None,
                        success: None,
                        duration_ms: None,
                    },
                })
                .expect("dynamic tool item/started should serialize"),
            ),
        }),
    )
    .await;

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-dynamic-tool-call-request".to_string()),
            method: "item/tool/call".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": "turn-remote-workflow",
                "callId": "tool-call-remote-workflow",
                "tool": "image-edit",
                "arguments": {
                    "prompt": format!("Sharpen image for {thread_id}"),
                    "strength": 0.5,
                },
            })),
            trace: None,
        }),
    )
    .await;
    let JSONRPCMessage::Response(dynamic_tool_response) = read_websocket_message(websocket).await
    else {
        panic!("expected dynamic tool call response");
    };
    pretty_assertions::assert_eq!(
        dynamic_tool_response.id,
        RequestId::String("shared-dynamic-tool-call-request".to_string())
    );
    pretty_assertions::assert_eq!(
        dynamic_tool_response.result,
        serde_json::json!({
            "contentItems": [{
                "type": "inputText",
                "text": format!("tool output for {thread_id}"),
            }],
            "success": true,
        })
    );

    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "serverRequest/resolved".to_string(),
            params: Some(
                serde_json::to_value(ServerRequestResolvedNotification {
                    thread_id: thread_id.to_string(),
                    request_id: RequestId::String("shared-dynamic-tool-call-request".to_string()),
                })
                .expect("dynamic tool serverRequest/resolved should serialize"),
            ),
        }),
    )
    .await;

    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/completed".to_string(),
            params: Some(
                serde_json::to_value(ItemCompletedNotification {
                    completed_at_ms: 0,
                    thread_id: thread_id.to_string(),
                    turn_id: "turn-remote-workflow".to_string(),
                    item: ThreadItem::DynamicToolCall {
                        namespace: None,
                        id: "tool-call-remote-workflow".to_string(),
                        tool: "image-edit".to_string(),
                        arguments: serde_json::json!({
                            "prompt": format!("Sharpen image for {thread_id}"),
                            "strength": 0.5,
                        }),
                        status: DynamicToolCallStatus::Completed,
                        content_items: Some(vec![DynamicToolCallOutputContentItem::InputText {
                            text: format!("tool output for {thread_id}"),
                        }]),
                        success: Some(true),
                        duration_ms: Some(7),
                    },
                })
                .expect("dynamic tool item/completed should serialize"),
            ),
        }),
    )
    .await;

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-permissions-request".to_string()),
            method: "item/permissions/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": "turn-remote-workflow",
                "itemId": "perm-remote-workflow",
                "startedAtMs": 0,
                "cwd": "/tmp/remote-project",
                "reason": "Need wider permissions",
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
    let JSONRPCMessage::Response(permissions_response) = read_websocket_message(websocket).await
    else {
        panic!("expected permissions approval response");
    };
    pretty_assertions::assert_eq!(
        permissions_response.id,
        RequestId::String("shared-permissions-request".to_string())
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

    write_websocket_message(
        websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::String("shared-chatgpt-refresh-request".to_string()),
            method: "account/chatgptAuthTokens/refresh".to_string(),
            params: Some(serde_json::json!({
                "reason": "unauthorized",
                "previousAccountId": previous_account_id,
            })),
            trace: None,
        }),
    )
    .await;
    let refresh_message = read_websocket_message(websocket).await;
    let JSONRPCMessage::Response(refresh_response) = refresh_message else {
        panic!("expected chatgpt refresh response, got {refresh_message:?}");
    };
    pretty_assertions::assert_eq!(
        refresh_response.id,
        RequestId::String("shared-chatgpt-refresh-request".to_string())
    );
    pretty_assertions::assert_eq!(
        refresh_response.result,
        serde_json::json!({
            "accessToken": format!("access-token-{previous_account_id}"),
            "chatgptAccountId": previous_account_id,
            "chatgptPlanType": "pro",
        })
    );

    if legacy_approval_exercise == LegacyApprovalExercise::Exercise {
        write_websocket_message(
            websocket,
            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                id: RequestId::String("shared-legacy-exec-approval".to_string()),
                method: "execCommandApproval".to_string(),
                params: Some(serde_json::json!({
                    "conversationId": thread_id,
                    "callId": "legacy-exec-call",
                    "approvalId": "legacy-exec-approval",
                    "command": ["echo", thread_id],
                    "cwd": preview,
                    "reason": "Need legacy command approval",
                    "parsedCmd": [{
                        "type": "unknown",
                        "cmd": format!("echo {thread_id}"),
                    }],
                })),
                trace: None,
            }),
        )
        .await;
        let legacy_exec_message = read_websocket_message(websocket).await;
        let JSONRPCMessage::Response(legacy_exec_response) = legacy_exec_message else {
            panic!("expected legacy exec approval response, got {legacy_exec_message:?}");
        };
        pretty_assertions::assert_eq!(
            legacy_exec_response.id,
            RequestId::String("shared-legacy-exec-approval".to_string())
        );
        pretty_assertions::assert_eq!(
            legacy_exec_response.result,
            serde_json::to_value(ExecCommandApprovalResponse {
                decision: ReviewDecision::Approved,
            })
            .expect("legacy exec approval response should serialize")
        );

        write_websocket_message(
            websocket,
            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                id: RequestId::String("shared-legacy-patch-approval".to_string()),
                method: "applyPatchApproval".to_string(),
                params: Some(serde_json::json!({
                    "conversationId": thread_id,
                    "callId": "legacy-patch-call",
                    "fileChanges": {
                        "README.md": {
                            "type": "add",
                            "content": format!("hello from {thread_id}\\n"),
                        },
                    },
                    "reason": "Need legacy patch approval",
                    "grantRoot": preview,
                })),
                trace: None,
            }),
        )
        .await;
        let legacy_patch_message = read_websocket_message(websocket).await;
        let JSONRPCMessage::Response(legacy_patch_response) = legacy_patch_message else {
            panic!("expected legacy apply-patch approval response, got {legacy_patch_message:?}");
        };
        pretty_assertions::assert_eq!(
            legacy_patch_response.id,
            RequestId::String("shared-legacy-patch-approval".to_string())
        );
        pretty_assertions::assert_eq!(
            legacy_patch_response.result,
            serde_json::to_value(ApplyPatchApprovalResponse {
                decision: ReviewDecision::ApprovedForSession,
            })
            .expect("legacy apply-patch approval response should serialize")
        );
    }

    while let Some(frame) = websocket.next().await {
        match frame.expect("follow-up frame should decode") {
            Message::Close(_) => break,
            Message::Ping(payload) => {
                websocket
                    .send(Message::Pong(payload))
                    .await
                    .expect("pong should send");
            }
            Message::Text(_) | Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }
}
