use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_reconnectable_request(
    method: &'static str,
    result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
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
                    if request.method == "configRequirements/read" {
                        assert_eq!(request.params, None);
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "requirements": null,
                                    }),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                        continue;
                    }
                    assert_eq!(request.method, method);
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: result.clone(),
                            }))
                            .expect("reconnectable response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
    method: &'static str,
    params: serde_json::Value,
    result: serde_json::Value,
) -> String {
    start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
            method,
            Some(params),
            result,
        )
        .await
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
    method: &'static str,
    params: Option<serde_json::Value>,
    result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
            let params = params.clone();
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
                        assert_eq!(request.method, method);
                        assert_json_params_eq(request.params, params.clone());
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: result.clone(),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_request_with_recording(
    method: &'static str,
    result: serde_json::Value,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
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
                    if request.method == "configRequirements/read" {
                        assert_eq!(request.params, None);
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "requirements": null,
                                    }),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                        continue;
                    }
                    assert_eq!(request.method, method);
                    requests.lock().await.push(request.method.clone());
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: result.clone(),
                            }))
                            .expect("reconnectable response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable response should send");
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
    method: &'static str,
    result: serde_json::Value,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
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
                        assert_eq!(request.method, method);
                        requests.lock().await.push(request.method.clone());
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: result.clone(),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}
