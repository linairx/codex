use super::embedded_test_support_reconnect_common::respond_to_reconnect_bootstrap_request;
use super::*;

pub(crate) async fn start_mock_remote_multi_connection_disconnect_after_thread_start_server(
    thread_id: &'static str,
    preview: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                1 => {
                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        if respond_to_reconnect_bootstrap_request(&mut websocket, &request).await {
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
                                break;
                            }
                            method => panic!("unexpected request method: {method}"),
                        }
                    }
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}
