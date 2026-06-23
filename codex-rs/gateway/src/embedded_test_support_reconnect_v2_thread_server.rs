use super::*;

#[path = "embedded_test_support_reconnect_v2_thread_server_turn_start.rs"]
mod embedded_test_support_reconnect_v2_thread_server_turn_start;

pub(crate) use self::embedded_test_support_reconnect_v2_thread_server_turn_start::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_thread_server(
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
                    _ => {
                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            tracing::info!(
                                method = %request.method,
                                "realtime reconnect helper received request",
                            );
                            if handle_thread_server_turn_start_request(
                                &mut websocket,
                                thread_id,
                                &request,
                            )
                            .await
                            {
                                continue;
                            }
                            match request.method.as_str() {
                                "thread/start" => {
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
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Request(
                                            codex_app_server_protocol::JSONRPCRequest {
                                                id: RequestId::String(
                                                    "srv-user-input-after-reconnect"
                                                        .to_string(),
                                                ),
                                                method: "item/tool/requestUserInput".to_string(),
                                                params: Some(
                                                    serde_json::to_value(
                                                        codex_app_server_protocol::ToolRequestUserInputParams {
                                                            auto_resolution_ms: None,
                                                            thread_id: thread_id.to_string(),
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
                                            },
                                        ),
                                    )
                                    .await;
                                    let JSONRPCMessage::Response(response) =
                                        read_websocket_message(&mut websocket).await
                                    else {
                                        panic!("expected server request response");
                                    };
                                    assert_eq!(
                                        response.id,
                                        RequestId::String(
                                            "srv-user-input-after-reconnect".to_string()
                                        )
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
                                                params: Some(serde_json::to_value(
                                                    ServerRequestResolvedNotification {
                                                        thread_id: thread_id.to_string(),
                                                        request_id: RequestId::String(
                                                            "srv-user-input-after-reconnect"
                                                                .to_string(),
                                                        ),
                                                    },
                                                )
                                                .expect(
                                                    "user input serverRequest/resolved should serialize",
                                                )),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/read" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "thread": mock_thread(thread_id, preview),
                                                "turns": [],
                                            }),
                                        }),
                                    )
                                    .await;
                                }
                                "thread/list" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "data": [mock_thread(thread_id, preview)],
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
