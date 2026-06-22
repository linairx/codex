use super::*;

#[path = "embedded_test_support_multi_worker_server_requests_session.rs"]
mod embedded_test_support_multi_worker_server_requests_session;

pub(crate) use self::embedded_test_support_multi_worker_server_requests_session::*;

#[path = "embedded_test_support_multi_worker_server_requests_concurrent.rs"]
mod embedded_test_support_multi_worker_server_requests_concurrent;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_server_requests_concurrent::*;

pub(crate) async fn start_mock_remote_multi_connection_server_request_server(
    thread_id: &'static str,
    preview: &'static str,
    expected_answer: &'static str,
    previous_account_id: &'static str,
    legacy_approval_exercise: LegacyApprovalExercise,
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
                drive_mock_remote_multi_connection_server_request_session(
                    &mut websocket,
                    thread_id,
                    preview,
                    expected_answer,
                    previous_account_id,
                    legacy_approval_exercise,
                )
                .await;
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_reconnecting_v2_multi_connection_server_request_server(
    thread_id: &'static str,
    preview: &'static str,
    expected_answer: &'static str,
    previous_account_id: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..4 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                    drive_mock_remote_multi_connection_server_request_session(
                        &mut websocket,
                        thread_id,
                        preview,
                        expected_answer,
                        previous_account_id,
                        LegacyApprovalExercise::Skip,
                    )
                    .await;
                    while let Some(message) = websocket.next().await {
                        match message {
                            Ok(Message::Close(_)) => break,
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_mutation_server(
    thread_id: &'static str,
    preview: &'static str,
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
                let mut thread_name: Option<String> = None;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "thread/start" => {
                            let thread =
                                mock_thread_with_name(thread_id, preview, thread_name.as_deref());
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({
                                        "thread": thread,
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
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "thread": thread,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/name/set" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/name/set should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
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
                                    id: request.id.clone(),
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
                            continue;
                        }
                        "thread/memoryMode/set" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/memoryMode/set should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            let mode = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("mode"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/memoryMode/set should include mode");
                            assert!(matches!(mode, "enabled" | "disabled"));
                            serde_json::json!({})
                        }
                        "thread/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            pretty_assertions::assert_eq!(requested_thread_id, thread_id);
                            serde_json::json!({
                                "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
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
                            })
                        }
                        "thread/list" => serde_json::json!({
                            "data": [mock_thread_with_name(thread_id, preview, thread_name.as_deref())],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
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

pub(crate) fn assert_remote_client_shutdown(result: std::io::Result<()>) {
    match result {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(err) => panic!("shutdown should complete: {err:?}"),
    }
}
