use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_realtime_server(
    thread_id: &'static str,
    preview: &'static str,
    session_id: &'static str,
    transcript_delta: &'static str,
    transcript_done: &'static str,
) -> String {
    start_reconnecting_v2_multi_connection_realtime_server_with_voices(
        thread_id,
        preview,
        session_id,
        transcript_delta,
        transcript_done,
        RealtimeVoicesList::builtin(),
    )
    .await
}

pub(crate) async fn start_reconnecting_v2_multi_connection_realtime_server_with_voices(
    thread_id: &'static str,
    preview: &'static str,
    session_id: &'static str,
    transcript_delta: &'static str,
    transcript_done: &'static str,
    realtime_voices: RealtimeVoicesList,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0.. {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let realtime_voices = realtime_voices.clone();
            tracing::info!(
                connection_index,
                "realtime reconnect helper accepted connection"
            );
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            match connection_index {
                0 => {
                    close_during_initialize(&mut websocket).await;
                }
                1 => {
                    close_during_initialize(&mut websocket).await;
                }
                _ => {
                    tokio::spawn(async move {
                        tracing::info!(
                            connection_index,
                            "realtime reconnect helper waiting for initialize"
                        );
                        expect_remote_initialize(&mut websocket).await;
                        tracing::info!(connection_index, "realtime reconnect helper initialized");
                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            tracing::info!(
                                method = %request.method,
                                "realtime reconnect helper received request",
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
                                "thread/realtime/listVoices" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "voices": serde_json::to_value(&realtime_voices)
                                                    .expect("realtime voices should serialize"),
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
                                }
                                "thread/realtime/start" => {
                                    let params = request
                                        .params
                                        .as_ref()
                                        .expect("realtime start params should exist");
                                    assert_eq!(params["threadId"], thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/started".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "realtimeSessionId": session_id,
                                                    "version": "v2",
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/itemAdded".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "item": {
                                                        "type": "message",
                                                        "id": format!("item-{thread_id}"),
                                                    },
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/realtime/appendText" => {
                                    let params = request
                                        .params
                                        .as_ref()
                                        .expect("realtime appendText params should exist");
                                    assert_eq!(params["threadId"], thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/transcript/delta"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "role": "assistant",
                                                    "delta": transcript_delta,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/transcript/done"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "role": "assistant",
                                                    "text": transcript_done,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/realtime/appendAudio" => {
                                    let params = request
                                        .params
                                        .as_ref()
                                        .expect("realtime appendAudio params should exist");
                                    assert_eq!(params["threadId"], thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/outputAudio/delta"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "audio": {
                                                        "data": params["audio"]["data"].clone(),
                                                        "sampleRate": 24000,
                                                        "numChannels": 1,
                                                        "samplesPerChannel": 3,
                                                        "itemId": params["audio"]["itemId"].clone(),
                                                    },
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/sdp".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Codex\r\n",
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/error".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "message": "realtime transport warning",
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                "thread/realtime/stop" => {
                                    let params = request
                                        .params
                                        .as_ref()
                                        .expect("realtime stop params should exist");
                                    assert_eq!(params["threadId"], thread_id);
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({}),
                                        }),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "thread/realtime/closed".to_string(),
                                                params: Some(serde_json::json!({
                                                    "threadId": thread_id,
                                                    "reason": "client requested stop",
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                method => panic!("unexpected request method: {method}"),
                            }
                        }
                    });
                }
            }
        }
    });
    format!("ws://{addr}")
}
