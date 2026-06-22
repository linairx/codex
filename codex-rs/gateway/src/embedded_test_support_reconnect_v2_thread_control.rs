use super::*;

pub(crate) async fn start_reconnecting_v2_single_worker_thread_control_server(
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
            connection_index += 1;
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
                    _ => {
                        let mut thread_name = None::<String>;
                        let review_thread_id = format!("{thread_id}-review");
                        let review_turn_id = format!("turn-review-{thread_id}");
                        let turn_id = format!("turn-{thread_id}");

                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
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
                                                "thread": mock_thread_with_name(
                                                    thread_id,
                                                    preview,
                                                    thread_name.as_deref(),
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
                                    let thread = if requested_thread_id == review_thread_id {
                                        mock_thread(&review_thread_id, preview)
                                    } else {
                                        assert_eq!(requested_thread_id, thread_id);
                                        mock_thread_with_name(
                                            thread_id,
                                            preview,
                                            thread_name.as_deref(),
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
                                }
                                method => panic!(
                                    "unexpected reconnectable single-worker thread method: {method}"
                                ),
                            }
                        }
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}
