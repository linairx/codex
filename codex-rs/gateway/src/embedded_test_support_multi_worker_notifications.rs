use super::*;

#[path = "embedded_test_support_multi_worker_notifications_warnings.rs"]
mod embedded_test_support_multi_worker_notifications_warnings;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_worker_notifications_warnings::*;

pub(crate) async fn start_mock_remote_multi_connection_skills_changed_server() -> String {
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
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                        method: "skills/changed".to_string(),
                        params: Some(serde_json::json!({})),
                    }),
                )
                .await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "skills/list" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "cwd": "/tmp/shared-repo",
                                            "skills": [],
                                            "errors": [],
                                        }],
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "skills/changed".to_string(),
                                        params: Some(serde_json::json!({})),
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
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_multi_connection_state_notification_server() -> String {
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
                for notification in [
                    serde_json::json!({
                        "method": "account/updated",
                        "params": {
                            "authMode": null,
                            "planType": null,
                        },
                    }),
                    serde_json::json!({
                        "method": "account/rateLimits/updated",
                        "params": {
                            "rateLimits": {
                                "limitId": null,
                                "limitName": null,
                                "primary": null,
                                "secondary": null,
                                "credits": null,
                                "planType": null,
                                "rateLimitReachedType": null,
                            },
                        },
                    }),
                    serde_json::json!({
                        "method": "app/list/updated",
                        "params": {
                            "data": [{
                                "id": "calendar",
                                "name": "calendar",
                                "description": null,
                                "logoUrl": null,
                                "logoUrlDark": null,
                                "distributionChannel": null,
                                "branding": null,
                                "appMetadata": null,
                                "labels": null,
                                "installUrl": null,
                            }],
                        },
                    }),
                    serde_json::json!({
                        "method": "account/login/completed",
                        "params": {
                            "loginId": null,
                            "success": true,
                            "error": null,
                        },
                    }),
                    serde_json::json!({
                        "method": "mcpServer/oauthLogin/completed",
                        "params": {
                            "name": "calendar-mcp",
                            "success": true,
                            "error": null,
                        },
                    }),
                    serde_json::json!({
                        "method": "mcpServer/startupStatus/updated",
                        "params": {
                            "name": "calendar-mcp",
                            "status": "ready",
                            "error": null,
                        },
                    }),
                    serde_json::json!({
                        "method": "warning",
                        "params": {
                            "threadId": null,
                            "message": "shared warning",
                        },
                    }),
                    serde_json::json!({
                        "method": "configWarning",
                        "params": {
                            "summary": "shared config warning",
                            "details": "check your shared config",
                        },
                    }),
                    serde_json::json!({
                        "method": "deprecationNotice",
                        "params": {
                            "summary": "shared deprecation notice",
                            "details": "update the shared workflow",
                        },
                    }),
                    serde_json::json!({
                        "method": "windows/worldWritableWarning",
                        "params": {
                            "samplePaths": ["C:\\shared-temp"],
                            "extraCount": 2,
                            "failedScan": false,
                        },
                    }),
                    serde_json::json!({
                        "method": "windowsSandbox/setupCompleted",
                        "params": {
                            "mode": "unelevated",
                            "success": true,
                            "error": null,
                        },
                    }),
                ] {
                    write_websocket_message(
                        &mut websocket,
                        serde_json::from_value::<JSONRPCMessage>(notification)
                            .expect("notification should decode"),
                    )
                    .await;
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_reconnecting_v2_single_worker_connection_state_notification_server()
-> String {
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
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            match current_connection_index {
                0 => {
                    close_during_initialize(&mut websocket).await;
                }
                1 => {
                    close_during_initialize(&mut websocket).await;
                }
                _ => {
                    tokio::spawn(async move {
                        expect_remote_initialize(&mut websocket).await;
                        tokio::time::sleep(
                            super::embedded_test_support_reconnect_common::RECONNECT_NOTIFICATION_DELAY,
                        )
                        .await;

                        for notification in [
                            serde_json::json!({
                                "method": "account/updated",
                                "params": {
                                    "authMode": null,
                                    "planType": null,
                                },
                            }),
                            serde_json::json!({
                                "method": "account/rateLimits/updated",
                                "params": {
                                    "rateLimits": {
                                        "limitId": null,
                                        "limitName": null,
                                        "primary": null,
                                        "secondary": null,
                                        "credits": null,
                                        "planType": null,
                                        "rateLimitReachedType": null,
                                    },
                                },
                            }),
                            serde_json::json!({
                                "method": "app/list/updated",
                                "params": {
                                    "data": [{
                                        "id": "calendar",
                                        "name": "calendar",
                                        "description": null,
                                        "logoUrl": null,
                                        "logoUrlDark": null,
                                        "distributionChannel": null,
                                        "branding": null,
                                        "appMetadata": null,
                                        "labels": null,
                                        "installUrl": null,
                                    }],
                                },
                            }),
                            serde_json::json!({
                                "method": "account/login/completed",
                                "params": {
                                    "loginId": null,
                                    "success": true,
                                    "error": null,
                                },
                            }),
                            serde_json::json!({
                                "method": "mcpServer/oauthLogin/completed",
                                "params": {
                                    "name": "calendar-mcp",
                                    "success": true,
                                    "error": null,
                                },
                            }),
                            serde_json::json!({
                                "method": "mcpServer/startupStatus/updated",
                                "params": {
                                    "name": "calendar-mcp",
                                    "status": "ready",
                                    "error": null,
                                },
                            }),
                            serde_json::json!({
                                "method": "warning",
                                "params": {
                                    "threadId": null,
                                    "message": "shared warning",
                                },
                            }),
                            serde_json::json!({
                                "method": "configWarning",
                                "params": {
                                    "summary": "shared config warning",
                                    "details": "check your shared config",
                                },
                            }),
                            serde_json::json!({
                                "method": "deprecationNotice",
                                "params": {
                                    "summary": "shared deprecation notice",
                                    "details": "update the shared workflow",
                                },
                            }),
                            serde_json::json!({
                                "method": "windows/worldWritableWarning",
                                "params": {
                                    "samplePaths": ["C:\\shared-temp"],
                                    "extraCount": 2,
                                    "failedScan": false,
                                },
                            }),
                            serde_json::json!({
                                "method": "windowsSandbox/setupCompleted",
                                "params": {
                                    "mode": "unelevated",
                                    "success": true,
                                    "error": null,
                                },
                            }),
                        ] {
                            write_websocket_message(
                                &mut websocket,
                                serde_json::from_value::<JSONRPCMessage>(notification)
                                    .expect("notification should decode"),
                            )
                            .await;
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
                                Message::Text(_)
                                | Message::Binary(_)
                                | Message::Pong(_)
                                | Message::Frame(_) => {}
                            }
                        }
                    });
                }
            }
        }
    });
    format!("ws://{addr}")
}
