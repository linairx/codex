use super::*;

pub(crate) async fn start_reconnecting_v2_bootstrap_refresh_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let _plugin_installed = false;
        for connection_index in 0..3 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            expect_remote_initialize(&mut websocket).await;
            match connection_index {
                0 => {
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
                2 => loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "account/read" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "account": {
                                            "type": "chatgpt",
                                            "email": "gateway@example.com",
                                            "planType": "pro",
                                        },
                                        "requiresOpenaiAuth": false,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "account/rateLimits/read" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "rateLimits": {
                                            "limitId": "codex",
                                            "limitName": "Codex",
                                            "primary": {
                                                "usedPercent": 42,
                                                "windowDurationMins": 300,
                                                "resetsAt": 1_700_000_000,
                                            },
                                            "secondary": null,
                                            "credits": null,
                                            "planType": "pro",
                                            "rateLimitReachedType": null,
                                        },
                                        "rateLimitsByLimitId": {
                                            "codex": {
                                                "limitId": "codex",
                                                "limitName": "Codex",
                                                "primary": {
                                                    "usedPercent": 42,
                                                    "windowDurationMins": 300,
                                                    "resetsAt": 1_700_000_000,
                                                },
                                                "secondary": null,
                                                "credits": null,
                                                "planType": "pro",
                                                "rateLimitReachedType": null,
                                            }
                                        },
                                    }),
                                }),
                            )
                            .await;
                        }
                        "model/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "id": "openai/gpt-5",
                                            "model": "gpt-5",
                                            "upgrade": null,
                                            "upgradeInfo": null,
                                            "availabilityNux": null,
                                            "displayName": "GPT-5",
                                            "description": "Recovered single-worker model",
                                            "hidden": false,
                                            "supportedReasoningEfforts": [{
                                                "reasoningEffort": "medium",
                                                "description": "Balanced",
                                            }],
                                            "defaultReasoningEffort": "medium",
                                            "inputModalities": ["text"],
                                            "supportsPersonality": false,
                                            "additionalSpeedTiers": [],
                                            "isDefault": true,
                                        }],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "externalAgentConfig/detect" => {
                            assert_eq!(
                                request.params,
                                Some(serde_json::json!({
                                    "includeHome": true,
                                    "cwds": ["/tmp/reconnected-project"],
                                })),
                            );
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "items": [{
                                            "itemType": "PLUGINS",
                                            "description": "reconnected repo config",
                                            "cwd": "/tmp/reconnected-project",
                                            "details": null,
                                        }],
                                    }),
                                }),
                            )
                            .await;
                        }
                        "app/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "id": "reconnected-app",
                                            "name": "Reconnected App",
                                            "description": "Recovered single-worker app",
                                            "installUrl": null,
                                            "displayName": "Recovered App",
                                            "shortDescription": "Recovered app list entry",
                                            "logoUrl": null,
                                            "bgColor": null,
                                            "createdAt": null,
                                            "deprecationReason": null,
                                        }],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "plugin/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "marketplaces": [{
                                            "id": "reconnected-marketplace",
                                            "name": "Recovered Marketplace",
                                            "description": "Recovered plugin list",
                                            "logoUrl": null,
                                            "apps": [{
                                                "id": "reconnected-app",
                                                "name": "Recovered App",
                                                "description": "Recovered app list entry",
                                                "installUrl": null,
                                                "displayName": "Recovered App",
                                                "shortDescription": "Recovered app list entry",
                                                "logoUrl": null,
                                                "bgColor": null,
                                                "createdAt": null,
                                                "deprecationReason": null,
                                            }],
                                        }],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/read" => {
                            let thread_id = request
                                .params
                                .as_ref()
                                .expect("thread/read params should exist")["id"]
                                .as_str()
                                .expect("thread id should be string");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": {
                                            "id": thread_id,
                                            "status": "running",
                                            "title": "Recovered thread",
                                            "entries": [],
                                            "workingDirectory": null,
                                            "rootDir": null,
                                            "startedAt": null,
                                            "completedAt": null,
                                        }
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
                                        "data": [{
                                            "id": "reconnected-thread",
                                            "status": "running",
                                            "title": "Recovered thread",
                                            "entries": [],
                                            "workingDirectory": null,
                                            "rootDir": null,
                                            "startedAt": null,
                                            "completedAt": null,
                                        }],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/branch" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "threadId": "reconnected-thread",
                                        "branchedThreadId": "reconnected-thread-branch",
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/turns/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/turns/start" => {
                            let thread_id = request
                                .params
                                .as_ref()
                                .expect("turn start params should exist")["threadId"]
                                .as_str()
                                .expect("thread id should be string");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turnId": "reconnected-turn",
                                        "threadId": thread_id,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/turns/stop" => {
                            let thread_id = request
                                .params
                                .as_ref()
                                .expect("turn stop params should exist")["threadId"]
                                .as_str()
                                .expect("thread id should be string");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "threadId": thread_id,
                                        "turnId": "reconnected-turn",
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/realtime/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [],
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/realtime/start" => {
                            let thread_id = request
                                .params
                                .as_ref()
                                .expect("realtime start params should exist")["threadId"]
                                .as_str()
                                .expect("thread id should be string");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "threadId": thread_id,
                                        "realtimeId": "reconnected-realtime",
                                    }),
                                }),
                            )
                            .await;
                        }
                        "thread/realtime/stop" => {
                            let thread_id = request
                                .params
                                .as_ref()
                                .expect("realtime stop params should exist")["threadId"]
                                .as_str()
                                .expect("thread id should be string");
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
                },
                _ => unreachable!("unexpected reconnect connection index"),
            }
        }
    });
    format!("ws://{addr}")
}
