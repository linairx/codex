use super::*;

pub(crate) async fn start_mock_remote_warning_notification_server() -> String {
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
                        method: "warning".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": null,
                            "message": "Gateway remote warning",
                        })),
                    }),
                )
                .await;

                tokio::time::sleep(Duration::from_secs(5)).await;
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_config_warning_notification_server() -> String {
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
                        method: "configWarning".to_string(),
                        params: Some(serde_json::json!({
                            "summary": "Gateway remote config warning",
                            "details": "Remote workers should refresh their config.",
                        })),
                    }),
                )
                .await;

                tokio::time::sleep(Duration::from_secs(5)).await;
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_deprecation_notice_notification_server() -> String {
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
                        method: "deprecationNotice".to_string(),
                        params: Some(serde_json::json!({
                            "summary": "Gateway deprecated flow",
                            "details": "Use the gateway replacement flow.",
                        })),
                    }),
                )
                .await;

                tokio::time::sleep(Duration::from_secs(5)).await;
            });
        }
    });
    format!("ws://{addr}")
}
