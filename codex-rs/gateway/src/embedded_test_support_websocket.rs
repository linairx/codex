use super::*;

pub(crate) struct MockRemoteServerOptions {
    pub(crate) expected_auth_token: Option<String>,
    pub(crate) thread_id: &'static str,
    pub(crate) preview: &'static str,
    pub(crate) close_after_first_request: bool,
}

pub(crate) async fn start_mock_remote_server_with_options(
    options: MockRemoteServerOptions,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let expected_auth_token = options.expected_auth_token.clone();
        let thread_id = options.thread_id;
        let preview = options.preview;
        let close_after_first_request = options.close_after_first_request;
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                let provided_auth_token = request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let expected_header = expected_auth_token
                    .as_ref()
                    .map(|token| format!("Bearer {token}"));
                pretty_assertions::assert_eq!(provided_auth_token, expected_header);
                Ok(response)
            },
        )
        .await
        .expect("websocket upgrade should succeed");

        expect_remote_initialize(&mut websocket).await;
        let mut handled_requests = 0usize;
        loop {
            let request = read_websocket_request(&mut websocket).await;
            let result = match request.method.as_str() {
                "configRequirements/read" => serde_json::json!({
                    "requirements": null,
                }),
                "thread/start" => serde_json::json!({
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
                "thread/read" => serde_json::json!({
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
                "thread/list" => serde_json::json!({
                    "data": [mock_thread(thread_id, preview)],
                    "nextCursor": null,
                    "backwardsCursor": null,
                }),
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
            handled_requests += 1;
            if close_after_first_request && handled_requests == 1 {
                websocket
                    .close(None)
                    .await
                    .expect("close frame should send");
                break;
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) fn mock_thread(thread_id: &str, preview: &str) -> serde_json::Value {
    mock_thread_with_name(thread_id, preview, None)
}

pub(crate) fn mock_thread_with_path(
    thread_id: &str,
    preview: &str,
    path: Option<&str>,
) -> serde_json::Value {
    mock_thread_with_name_and_path(thread_id, preview, None, path)
}

pub(crate) fn mock_thread_with_name(
    thread_id: &str,
    preview: &str,
    name: Option<&str>,
) -> serde_json::Value {
    mock_thread_with_name_and_path(thread_id, preview, name, None)
}

pub(crate) fn mock_thread_with_name_and_path(
    thread_id: &str,
    preview: &str,
    name: Option<&str>,
    path: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "id": thread_id,
        "sessionId": thread_id,
        "forkedFromId": null,
        "preview": preview,
        "ephemeral": true,
        "modelProvider": "openai",
        "createdAt": 1,
        "updatedAt": if thread_id.ends_with("-b") { 2 } else { 1 },
        "status": {
            "type": "idle"
        },
        "path": path,
        "cwd": preview,
        "cliVersion": "0.0.0-test",
        "source": "vscode",
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": name,
        "turns": [],
    })
}

pub(crate) fn mock_turn(turn_id: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": turn_id,
        "items": [],
        "status": status,
        "error": null,
        "startedAt": 1,
        "completedAt": if status == "completed" { Some(2) } else { None::<i64> },
        "durationMs": if status == "completed" { Some(1000) } else { None::<i64> },
    })
}

pub(crate) async fn expect_remote_initialize<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let JSONRPCMessage::Request(request) = read_websocket_message(websocket).await else {
        panic!("expected initialize request");
    };
    pretty_assertions::assert_eq!(request.method, "initialize");
    write_websocket_message(
        websocket,
        JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result: serde_json::json!({}),
        }),
    )
    .await;

    let JSONRPCMessage::Notification(notification) = read_websocket_message(websocket).await else {
        panic!("expected initialized notification");
    };
    pretty_assertions::assert_eq!(notification.method, "initialized");
}

pub(crate) async fn close_during_initialize<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    websocket
        .close(None)
        .await
        .expect("close frame should send");
}

pub(crate) async fn start_mock_exec_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener addr");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("connection should accept");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected initialize request");
        };
        pretty_assertions::assert_eq!(request.method, "initialize");
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "sessionId": "exec-session-1"
                }),
            }),
        )
        .await;

        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected initialized notification");
        };
        pretty_assertions::assert_eq!(notification.method, "initialized");

        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected environment info request");
        };
        pretty_assertions::assert_eq!(request.method, "environment/info");
        write_websocket_message(
            &mut websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "shell": {
                        "name": "sh",
                        "path": "/bin/sh",
                    }
                }),
            }),
        )
        .await;

        while websocket
            .next()
            .await
            .expect("exec-server websocket should stay open")
            .is_ok()
        {}
    });
    format!("ws://{addr}")
}

pub(crate) async fn read_websocket_message<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> JSONRPCMessage
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = websocket
            .next()
            .await
            .expect("frame should be available")
            .expect("frame should decode");
        match frame {
            Message::Text(text) => {
                return serde_json::from_str::<JSONRPCMessage>(&text)
                    .expect("text frame should be valid JSON-RPC");
            }
            Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
            Message::Ping(payload) => {
                websocket
                    .send(Message::Pong(payload))
                    .await
                    .expect("pong should send");
                continue;
            }
            Message::Close(frame) => panic!("unexpected close frame: {frame:?}"),
        }
    }
}

pub(crate) async fn read_websocket_request<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> codex_app_server_protocol::JSONRPCRequest
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        match read_websocket_message(websocket).await {
            JSONRPCMessage::Request(request) => return request,
            JSONRPCMessage::Notification(notification) if notification.method == "initialized" => {
                continue;
            }
            other => panic!("expected request, got {other:?}"),
        }
    }
}

pub(crate) async fn read_websocket_request_until_close<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Option<codex_app_server_protocol::JSONRPCRequest>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = websocket.next().await?.expect("frame should decode");
        match frame {
            Message::Text(text) => {
                let message = serde_json::from_str::<JSONRPCMessage>(&text)
                    .expect("text frame should be valid JSON-RPC");
                match message {
                    JSONRPCMessage::Request(request) => return Some(request),
                    JSONRPCMessage::Notification(notification)
                        if notification.method == "initialized" =>
                    {
                        continue;
                    }
                    other => panic!("expected request, got {other:?}"),
                }
            }
            Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {
                continue;
            }
            Message::Ping(payload) => {
                websocket
                    .send(Message::Pong(payload))
                    .await
                    .expect("pong should send");
                continue;
            }
            Message::Close(_) => return None,
        }
    }
}

pub(crate) async fn write_websocket_message<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
    message: JSONRPCMessage,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    websocket
        .send(Message::Text(
            serde_json::to_string(&message)
                .expect("message should serialize")
                .into(),
        ))
        .await
        .expect("message should send");
}
