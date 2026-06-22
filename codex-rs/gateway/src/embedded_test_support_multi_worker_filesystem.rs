use super::*;

pub(crate) async fn start_mock_remote_multi_connection_filesystem_operations_server(
    worker_label: &'static str,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    {
        let requests = requests.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let requests = requests.clone();
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        requests.lock().await.push(request.method.clone());
                        let result = match request.method.as_str() {
                            "fs/createDirectory" | "fs/writeFile" | "fs/copy" | "fs/remove" => {
                                serde_json::json!({})
                            }
                            "fs/readFile" => serde_json::json!({
                                "dataBase64": match worker_label {
                                    "worker-a" => "d29ya2VyLWEtZmlsZQ==",
                                    "worker-b" => "d29ya2VyLWItZmlsZQ==",
                                    other => panic!("unexpected worker label: {other}"),
                                },
                            }),
                            "fs/getMetadata" => serde_json::json!({
                                "isDirectory": false,
                                "isFile": true,
                                "isSymlink": false,
                                "createdAtMs": 0,
                                "modifiedAtMs": 0,
                            }),
                            "fs/readDirectory" => serde_json::json!({
                                "entries": [{
                                    "fileName": format!("{worker_label}-gateway.txt"),
                                    "isDirectory": false,
                                    "isFile": true,
                                }],
                            }),
                            "fuzzyFileSearch" => {
                                multi_worker_fuzzy_file_search_response(worker_label)
                            }
                            "fuzzyFileSearch/sessionStart" => serde_json::json!({}),
                            "fuzzyFileSearch/sessionUpdate" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fuzzyFileSearch/sessionUpdated".to_string(),
                                            params: Some(serde_json::json!({
                                                "sessionId": "multi-worker-fuzzy-session",
                                                "query": "gate",
                                                "files": [{
                                                    "root": "/tmp/worker-a-primary",
                                                    "path": "docs/gateway.md",
                                                    "match_type": "file",
                                                    "file_name": "gateway.md",
                                                    "score": 42,
                                                    "indices": [5, 6, 7, 8],
                                                }],
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                serde_json::json!({})
                            }
                            "fuzzyFileSearch/sessionStop" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fuzzyFileSearch/sessionCompleted".to_string(),
                                            params: Some(serde_json::json!({
                                                "sessionId": "multi-worker-fuzzy-session",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                serde_json::json!({})
                            }
                            "windowsSandbox/setupStart" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "started": true,
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "windowsSandbox/setupCompleted".to_string(),
                                            params: Some(serde_json::json!({
                                                "mode": "unelevated",
                                                "success": true,
                                                "error": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                continue;
                            }
                            method => panic!("unexpected request method: {method}"),
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
    }
    (format!("ws://{addr}"), requests)
}
