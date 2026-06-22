macro_rules! assert_eq {
    ($($tt:tt)*) => {
        ::std::assert_eq!($($tt)*)
    };
}

use super::embedded_test_support_reconnect_common::respond_to_reconnect_bootstrap_request;
use super::*;

pub(crate) async fn start_mock_remote_server(
    expected_auth_token: Option<String>,
    thread_id: &'static str,
    preview: &'static str,
) -> String {
    start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token,
        thread_id,
        preview,
        close_after_first_request: false,
    })
    .await
}

pub(crate) async fn start_mock_remote_server_for_thread_start_error(
    expected_auth_token: Option<String>,
    response_error: JSONRPCErrorError,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
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

        expect_remote_initialize(&mut websocket).await;
        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, "thread/start");
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Error(JSONRPCError {
                id: request.id,
                error: response_error,
            }),
        )
        .await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_idle_v2_sessions(
    expected_auth_token: Option<String>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let expected_auth_token = expected_auth_token.clone();
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
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

                expect_remote_initialize(&mut websocket).await;
                tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_closes_before_initialize_response() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0.. {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
                else {
                    panic!("expected initialize request");
                };
                assert_eq!(request.method, "initialize");

                if connection_index == 0 {
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({}),
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Notification(notification) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");
                    sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                } else {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_slow_client_timeout() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let warning_body = "w".repeat(8 * 1024 * 1024);
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let warning_body = warning_body.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                        id: RequestId::String("pending-chatgpt-refresh".to_string()),
                        method: "account/chatgptAuthTokens/refresh".to_string(),
                        params: Some(serde_json::json!({
                            "reason": "unauthorized",
                            "previousAccountId": "acct-visible",
                        })),
                        trace: None,
                    }),
                )
                .await;

                sleep(Duration::from_millis(50)).await;

                for sequence in 0..64 {
                    let large_warning_payload =
                        serde_json::to_string(&JSONRPCMessage::Notification(
                            codex_app_server_protocol::JSONRPCNotification {
                                method: "warning".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": null,
                                    "message": format!("warning-{sequence}-{warning_body}"),
                                })),
                            },
                        ))
                        .expect("warning notification should serialize");

                    if websocket
                        .send(Message::Text(large_warning_payload.clone().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_reconnecting_mock_remote_server(
    expected_auth_token: Option<String>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
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

            expect_remote_initialize(&mut websocket).await;
            loop {
                let request = read_websocket_request(&mut websocket).await;
                if respond_to_reconnect_bootstrap_request(&mut websocket, &request).await {
                    continue;
                }
                match request.method.as_str() {
                    "thread/start" => {
                        let thread_id = if connection_index == 0 {
                            "thread-worker-a-1"
                        } else {
                            "thread-worker-a-2"
                        };
                        let preview = if connection_index == 0 {
                            "/tmp/worker-a-1"
                        } else {
                            "/tmp/worker-a-2"
                        };
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
                        if connection_index == 0 {
                            websocket
                                .close(None)
                                .await
                                .expect("close frame should send");
                            break;
                        }
                    }
                    method => panic!("unexpected request method: {method}"),
                }
            }
        }
    });
    format!("ws://{addr}")
}
