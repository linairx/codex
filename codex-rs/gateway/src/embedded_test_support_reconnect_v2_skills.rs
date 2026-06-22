use super::embedded_test_support_reconnect_common::RECONNECT_NOTIFICATION_DELAY;
use super::embedded_test_support_reconnect_common::respond_to_reconnect_bootstrap_request;
use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_skills_changed_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..8 {
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
                2 | 3 => {
                    tokio::spawn(async move {
                        expect_remote_initialize(&mut websocket).await;
                        tokio::time::sleep(RECONNECT_NOTIFICATION_DELAY).await;

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

                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            let is_skills_list = request.method.as_str() == "skills/list";
                            if !respond_to_reconnect_bootstrap_request(&mut websocket, &request)
                                .await
                            {
                                panic!("unexpected request method: {}", request.method);
                            }
                            if is_skills_list {
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
                        }
                    });
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_reconnecting_v2_single_worker_skills_changed_server() -> String {
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
                        tokio::time::sleep(RECONNECT_NOTIFICATION_DELAY).await;

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

                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            let is_skills_list = request.method.as_str() == "skills/list";
                            if !respond_to_reconnect_bootstrap_request(&mut websocket, &request)
                                .await
                            {
                                panic!("unexpected request method: {}", request.method);
                            }
                            if is_skills_list {
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
                        }
                    });
                }
            }
        }
    });
    format!("ws://{addr}")
}
