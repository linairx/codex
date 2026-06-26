use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };
                    requests.lock().await.push(request.method.clone());
                    let result = match request.method.as_str() {
                        "fs/watch" => serde_json::json!({
                            "path": request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("path"))
                                .cloned()
                                .expect("fs/watch should include path"),
                        }),
                        "fs/unwatch" => serde_json::json!({}),
                        method => panic!("unexpected reconnectable fs method: {method}"),
                    };
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("reconnectable fs response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable fs response should send");
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_and_unwatch()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
                        let frame = frame.expect("frame should decode");
                        let Message::Text(text) = frame else {
                            continue;
                        };
                        let JSONRPCMessage::Request(request) =
                            serde_json::from_str(&text).expect("request should decode")
                        else {
                            continue;
                        };
                        requests.lock().await.push(request.method.clone());
                        let result = match request.method.as_str() {
                            "fs/watch" => serde_json::json!({
                                "path": request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("path"))
                                    .cloned()
                                    .expect("fs/watch should include path"),
                            }),
                            "fs/unwatch" => serde_json::json!({}),
                            method => panic!("unexpected reconnectable fs method: {method}"),
                        };
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result,
                                }))
                                .expect("reconnectable fs response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable fs response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_with_changed_notification()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        let request = read_websocket_request(&mut websocket).await;
                        assert_eq!(request.method, "fs/watch");
                        requests.lock().await.push(request.method.clone());
                        let params = request
                            .params
                            .clone()
                            .expect("fs/watch should include params");
                        let path = params
                            .get("path")
                            .cloned()
                            .expect("fs/watch should include path");
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "path": path,
                                    }),
                                }))
                                .expect("fs/watch response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("fs/watch response should send");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        send_remote_notification(
                            &mut websocket,
                            "fs/changed",
                            serde_json::json!({
                                "watchId": "watch-reconnected",
                                "changedPaths": ["/tmp/shared/project/.git/HEAD"],
                            }),
                        )
                        .await;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_app_list(
    apps: Vec<(&str, &str)>,
) -> String {
    let apps = apps
        .into_iter()
        .map(|(id, name)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "description": format!("{name} description"),
                "installUrl": null,
                "needsAuth": false,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "app/list",
        serde_json::json!({
            "data": apps,
            "nextCursor": null,
        }),
    )
    .await
}

pub(crate) fn reconnectable_model_json(
    id: &str,
    display_name: &str,
    is_default: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "model": id,
        "upgrade": null,
        "upgradeInfo": null,
        "availabilityNux": null,
        "displayName": display_name,
        "description": format!("{display_name} description"),
        "hidden": false,
        "defaultServiceTier": null,
        "serviceTiers": [],
        "supportedReasoningEfforts": [{
            "reasoningEffort": "medium",
            "description": "Balanced",
        }],
        "defaultReasoningEffort": "medium",
        "inputModalities": ["text"],
        "supportsPersonality": false,
        "additionalSpeedTiers": [],
        "isDefault": is_default,
    })
}

pub(crate) async fn start_mock_remote_server_for_paginated_model_list(
    pages: Vec<(Option<String>, serde_json::Value)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        while let Some(message) = websocket.next().await {
            let Message::Text(text) = message.expect("model/list request should decode") else {
                continue;
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("model/list request should deserialize")
            else {
                panic!("expected model/list request");
            };
            assert_eq!(request.method, "model/list");
            assert_eq!(
                request
                    .params
                    .as_ref()
                    .and_then(|params| params.get("limit")),
                Some(&Value::Null)
            );
            assert_eq!(
                request
                    .params
                    .as_ref()
                    .and_then(|params| params.get("includeHidden"))
                    .and_then(Value::as_bool),
                Some(true)
            );
            let cursor = request
                .params
                .as_ref()
                .and_then(|params| params.get("cursor"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let response = pages
                .iter()
                .find(|(page_cursor, _)| *page_cursor == cursor)
                .map(|(_, response)| response.clone())
                .unwrap_or_else(|| panic!("unexpected model/list cursor: {cursor:?}"));
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: response,
                    }))
                    .expect("model/list response should serialize")
                    .into(),
                ))
                .await
                .expect("model/list response should send");
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_mcp_server_status_list(
    names: Vec<&str>,
) -> String {
    let statuses = names
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken",
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "mcpServerStatus/list",
        serde_json::json!({
            "data": statuses,
            "nextCursor": null,
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_mcp_status_with_startup_notification()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        let request = read_websocket_request(&mut websocket).await;
                        assert_eq!(request.method, "mcpServerStatus/list");
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "name": "worker-b-mcp",
                                            "tools": {},
                                            "resources": [],
                                            "resourceTemplates": [],
                                            "authStatus": "bearerToken",
                                        }],
                                        "nextCursor": null,
                                    }),
                                }))
                                .expect("mcp status response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("mcp status response should send");
                        send_remote_notification(
                            &mut websocket,
                            "mcpServer/startupStatus/updated",
                            serde_json::json!({
                                "name": "worker-b-mcp",
                                "status": "ready",
                                "error": null,
                            }),
                        )
                        .await;
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}
