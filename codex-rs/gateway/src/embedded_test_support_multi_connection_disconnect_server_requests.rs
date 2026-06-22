use super::embedded_test_support_reconnect_common::respond_to_reconnect_bootstrap_request;
use super::*;

pub(crate) async fn start_mock_remote_multi_connection_disconnect_after_server_request_server()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                1 => {
                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        if respond_to_reconnect_bootstrap_request(&mut websocket, &request).await {
                            continue;
                        }
                        match request.method.as_str() {
                            "thread/start" => {
                                write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
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
                            id: RequestId::String("pending-user-input".to_string()),
                            method: "item/tool/requestUserInput".to_string(),
                            params: Some(
                                serde_json::to_value(
                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                        auto_resolution_ms: None,
                                        thread_id: "thread-worker-a".to_string(),
                                        turn_id: "turn-worker-a".to_string(),
                                        item_id: "tool-call-worker-a".to_string(),
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

                    sleep(Duration::from_millis(50)).await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_disconnect_after_server_request_response_server()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                1 => {
                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        if respond_to_reconnect_bootstrap_request(&mut websocket, &request).await {
                            continue;
                        }
                        match request.method.as_str() {
                            "thread/start" => {
                                write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
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
                            id: RequestId::String("pending-user-input".to_string()),
                            method: "item/tool/requestUserInput".to_string(),
                            params: Some(
                                serde_json::to_value(
                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                        auto_resolution_ms: None,
                                        thread_id: "thread-worker-a".to_string(),
                                        turn_id: "turn-worker-a".to_string(),
                                        item_id: "tool-call-worker-a".to_string(),
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
                        RequestId::String("pending-user-input".to_string())
                    );
                    pretty_assertions::assert_eq!(
                        response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": ["auto"],
                                },
                            },
                        })
                    );

                    sleep(Duration::from_millis(50)).await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_disconnect_after_connection_server_request_server()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                1 => {
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
                            id: RequestId::String("pending-chatgpt-refresh".to_string()),
                            method: "account/chatgptAuthTokens/refresh".to_string(),
                            params: Some(serde_json::json!({
                                "reason": "unauthorized",
                                "previousAccountId": "acct-worker-a",
                            })),
                            trace: None,
                        }),
                    )
                    .await;

                    sleep(Duration::from_millis(50)).await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_disconnect_after_connection_server_request_response_server()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                1 => {
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
                            id: RequestId::String("pending-chatgpt-refresh".to_string()),
                            method: "account/chatgptAuthTokens/refresh".to_string(),
                            params: Some(serde_json::json!({
                                "reason": "unauthorized",
                                "previousAccountId": "acct-worker-a",
                            })),
                            trace: None,
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Response(response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected chatgpt refresh response");
                    };
                    pretty_assertions::assert_eq!(
                        response.id,
                        RequestId::String("pending-chatgpt-refresh".to_string())
                    );
                    pretty_assertions::assert_eq!(
                        response.result,
                        serde_json::json!({
                            "accessToken": "access-token-worker-a",
                            "chatgptAccountId": "acct-worker-a",
                            "chatgptPlanType": "pro",
                        })
                    );

                    sleep(Duration::from_millis(50)).await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}
