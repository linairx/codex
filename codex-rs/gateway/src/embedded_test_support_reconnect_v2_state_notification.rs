use super::embedded_test_support_reconnect_common::RECONNECT_NOTIFICATION_DELAY;
use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_state_notification_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0.. {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                        expect_remote_initialize(&mut websocket).await;
                        tokio::time::sleep(RECONNECT_NOTIFICATION_DELAY).await;

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
