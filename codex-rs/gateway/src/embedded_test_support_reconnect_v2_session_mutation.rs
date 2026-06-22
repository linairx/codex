use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_session_mutation_server(
    worker_label: &'static str,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let received_methods = Arc::new(Mutex::new(Vec::new()));
    let received_methods_for_task = Arc::clone(&received_methods);
    tokio::spawn(async move {
        for connection_index in 0.. {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            expect_remote_initialize(&mut websocket).await;
            match connection_index {
                0 | 2 => {
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
                    let received_methods = Arc::clone(&received_methods_for_task);
                    tokio::spawn(async move {
                        while let Some(request) =
                            read_websocket_request_until_close(&mut websocket).await
                        {
                            received_methods.lock().await.push(request.method.clone());

                            let result = match request.method.as_str() {
                                "configRequirements/read" => serde_json::json!({
                                    "requirements": null,
                                }),
                                "externalAgentConfig/import" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "externalAgentConfig/import/completed"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "importId": "import-1",
                                                    "itemTypeResults": [],
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    serde_json::json!({
                                        "importId": "import-1",
                                    })
                                }
                                "config/value/write" | "config/batchWrite" => {
                                    serde_json::json!({
                                        "status": "ok",
                                        "version": worker_label,
                                        "filePath": "/tmp/shared/config.toml",
                                        "overriddenMetadata": null,
                                    })
                                }
                                "marketplace/add" => serde_json::json!({
                                    "marketplaceName": "shared-marketplace",
                                    "installedRoot": "/tmp/shared/marketplace",
                                    "alreadyAdded": false,
                                }),
                                "skills/config/write" => {
                                    assert_eq!(
                                        request
                                            .params
                                            .as_ref()
                                            .and_then(|params| params.get("enabled"))
                                            .and_then(serde_json::Value::as_bool),
                                        Some(true)
                                    );
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
                                    serde_json::json!({
                                        "effectiveEnabled": true,
                                    })
                                }
                                "experimentalFeature/enablement/set" => {
                                    let enablement = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("enablement"))
                                        .cloned()
                                        .expect("experimentalFeature/enablement/set should include enablement");
                                    serde_json::json!({
                                        "enablement": enablement,
                                    })
                                }
                                "config/mcpServer/reload" => serde_json::json!({}),
                                "fs/watch" => {
                                    let path = request
                                        .params
                                        .as_ref()
                                        .and_then(|params| params.get("path"))
                                        .cloned()
                                        .expect("fs/watch should include path");
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "fs/changed".to_string(),
                                                params: Some(serde_json::json!({
                                                    "watchId": "watch-shared",
                                                    "changedPaths": [
                                                        format!("/tmp/{worker_label}/shared-repo/.git/HEAD"),
                                                    ],
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    serde_json::json!({
                                        "path": path,
                                    })
                                }
                                "fs/unwatch" => serde_json::json!({}),
                                "memory/reset" => serde_json::json!({}),
                                "account/logout" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "account/updated".to_string(),
                                                params: Some(serde_json::json!({
                                                    "authMode": null,
                                                    "planType": null,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    serde_json::json!({})
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
            }
        }
    });
    (format!("ws://{addr}"), received_methods)
}
