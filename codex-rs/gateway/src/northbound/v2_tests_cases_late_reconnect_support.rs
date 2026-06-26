use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_review_start_then_thread_read() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let review_start = websocket
            .next()
            .await
            .expect("review/start request should exist")
            .expect("review/start request should decode");
        let Message::Text(review_start_text) = review_start else {
            panic!("expected review/start text frame");
        };
        let JSONRPCMessage::Request(review_start_request) =
            serde_json::from_str(&review_start_text).expect("review/start should decode")
        else {
            panic!("expected review/start request");
        };
        assert_eq!(review_start_request.method, "review/start");
        assert_json_params_eq(
            review_start_request.params,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "target": {
                    "type": "custom",
                    "instructions": "Review the current change",
                },
                "delivery": "detached",
            })),
        );
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: review_start_request.id,
                    result: serde_json::json!({
                        "turn": {
                            "id": "turn-review",
                            "items": [],
                            "status": "pending",
                            "error": null,
                            "startedAt": 1,
                            "completedAt": null,
                            "durationMs": null,
                        },
                        "reviewThreadId": "thread-review",
                    }),
                }))
                .expect("review/start response should serialize")
                .into(),
            ))
            .await
            .expect("review/start response should send");

        let thread_read = websocket
            .next()
            .await
            .expect("thread/read request should exist")
            .expect("thread/read request should decode");
        let Message::Text(thread_read_text) = thread_read else {
            panic!("expected thread/read text frame");
        };
        let JSONRPCMessage::Request(thread_read_request) =
            serde_json::from_str(&thread_read_text).expect("thread/read should decode")
        else {
            panic!("expected thread/read request");
        };
        assert_eq!(thread_read_request.method, "thread/read");
        assert_json_params_eq(
            thread_read_request.params,
            Some(serde_json::json!({
                "threadId": "thread-review",
                "includeTurns": false,
            })),
        );
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: thread_read_request.id,
                    result: serde_json::json!({
                        "thread": {
                            "id": "thread-review",
                            "name": "Detached review thread",
                        },
                    }),
                }))
                .expect("thread/read response should serialize")
                .into(),
            ))
            .await
            .expect("thread/read response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_thread_fork_and_read() -> String {
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
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
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
                        "thread/fork" => serde_json::json!({
                            "thread": {
                                "id": "thread-forked",
                                "name": "Forked thread",
                                "cwd": "/tmp/worker-b",
                            },
                            "model": "gpt-5",
                            "modelProvider": "openai",
                            "serviceTier": null,
                            "cwd": "/tmp/worker-b",
                            "instructionSources": [],
                            "approvalPolicy": "on-request",
                            "approvalsReviewer": "user",
                            "sandbox": { "type": "dangerFullAccess" },
                            "reasoningEffort": null,
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": "thread-forked",
                                "name": "Forked thread",
                                "cwd": "/tmp/worker-b",
                            },
                        }),
                        other => panic!("unexpected reconnectable thread method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_review_start_then_thread_read()
-> String {
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
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
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
                        "review/start" => serde_json::json!({
                            "turn": {
                                "id": "turn-review",
                                "items": [],
                                "status": "pending",
                                "error": null,
                                "startedAt": 1,
                                "completedAt": null,
                                "durationMs": null,
                            },
                            "reviewThreadId": "thread-review",
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": "thread-review",
                                "name": "Detached review thread",
                            },
                        }),
                        other => panic!("unexpected reconnectable review method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_review_start_then_thread_read()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
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
                            "review/start" => serde_json::json!({
                                "turn": {
                                    "id": "turn-review",
                                    "items": [],
                                    "status": "pending",
                                    "error": null,
                                    "startedAt": 1,
                                    "completedAt": null,
                                    "durationMs": null,
                                },
                                "reviewThreadId": "thread-review",
                            }),
                            "thread/read" => serde_json::json!({
                                "thread": {
                                    "id": "thread-review",
                                    "name": "Detached review thread",
                                },
                            }),
                            other => {
                                panic!("unexpected reconnectable review method: {other}")
                            }
                        };

                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result,
                                }))
                                .expect("response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_thread_list_and_read(
    thread_id: &str,
    thread_name: &str,
    cwd: &str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let thread_id = thread_id.to_string();
    let thread_name = thread_name.to_string();
    let cwd = cwd.to_string();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let thread_id = thread_id.clone();
            let thread_name = thread_name.clone();
            let cwd = cwd.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
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
                        "thread/list" => serde_json::json!({
                            "data": [{
                                "id": thread_id,
                                "sessionId": thread_id,
                                "forkedFromId": null,
                                "preview": "",
                                "ephemeral": true,
                                "modelProvider": "openai",
                                "createdAt": if thread_id == "thread-worker-a" { 1 } else { 2 },
                                "updatedAt": if thread_id == "thread-worker-a" { 1 } else { 2 },
                                "status": { "type": "idle" },
                                "path": null,
                                "cwd": cwd,
                                "cliVersion": "0.0.0-test",
                                "source": "cli",
                                "agentNickname": null,
                                "agentRole": null,
                                "gitInfo": null,
                                "name": thread_name,
                                "turns": [],
                            }],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": thread_id,
                                "name": thread_name,
                                "cwd": cwd,
                            },
                        }),
                        "thread/resume" => serde_json::json!({
                            "thread": {
                                "id": thread_id,
                                "name": thread_name,
                                "cwd": cwd,
                            },
                        }),
                        other => panic!("unexpected request method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) fn reconnectable_thread_list_entry_json(
    thread_id: &str,
    thread_name: &str,
    cwd: &str,
    timestamp: i64,
) -> serde_json::Value {
    serde_json::json!({
        "id": thread_id,
        "sessionId": thread_id,
        "forkedFromId": null,
        "preview": "",
        "ephemeral": true,
        "modelProvider": "openai",
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "status": { "type": "idle" },
        "path": null,
        "cwd": cwd,
        "cliVersion": "0.0.0-test",
        "source": "cli",
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": thread_name,
        "turns": [],
    })
}

pub(crate) async fn start_mock_remote_server_for_connection_notification(
    method: &str,
    params: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let method = method.to_string();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        let params = if method == "externalAgentConfig/import/completed" {
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            })
        } else {
            params
        };
        send_remote_notification(&mut websocket, &method, params).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_idle_session() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}
