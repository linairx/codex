use super::*;

pub(crate) async fn start_mock_remote_multi_connection_wrong_thread_read_server() -> String {
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

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "thread/start" => {
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    "00000000-0000-0000-0000-0000000000a1",
                                    "/tmp/worker-a",
                                    Some("/tmp/worker-a/rollout.jsonl"),
                                ),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/worker-a",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    "00000000-0000-0000-0000-0000000000a1",
                                    "/tmp/worker-a-wrong-read",
                                    Some("/tmp/worker-a/rollout.jsonl"),
                                ),
                            })
                        }
                        method => {
                            panic!("unexpected wrong-thread-read request method: {method}")
                        }
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}
