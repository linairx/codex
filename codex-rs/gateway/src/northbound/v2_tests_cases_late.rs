use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_late_notifications.rs"]
mod v2_tests_cases_late_notifications;

#[path = "v2_tests_cases_late_notification_dedupe.rs"]
mod v2_tests_cases_late_notification_dedupe;

#[path = "v2_tests_cases_late_route_diagnostics.rs"]
mod v2_tests_cases_late_route_diagnostics;

#[path = "v2_tests_cases_late_observability_cleanup.rs"]
mod v2_tests_cases_late_observability_cleanup;

#[path = "v2_tests_cases_late_observability_request_limits.rs"]
mod v2_tests_cases_late_observability_request_limits;

#[path = "v2_tests_cases_late_observability_delivery_failures.rs"]
mod v2_tests_cases_late_observability_delivery_failures;

#[path = "v2_tests_cases_late_observability_connection_pressure.rs"]
mod v2_tests_cases_late_observability_connection_pressure;

#[path = "v2_tests_cases_late_observability_pending_client.rs"]
mod v2_tests_cases_late_observability_pending_client;

#[path = "v2_tests_cases_late_observability_server_requests.rs"]
mod v2_tests_cases_late_observability_server_requests;

#[path = "v2_tests_cases_late_observability_notifications.rs"]
mod v2_tests_cases_late_observability_notifications;

#[path = "v2_tests_cases_late_observability_routing.rs"]
mod v2_tests_cases_late_observability_routing;
pub(crate) async fn start_mock_remote_server_expecting_forwarded_initialized() -> String {
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

        let frame = websocket
            .next()
            .await
            .expect("forwarded initialized frame should exist")
            .expect("forwarded initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected forwarded initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("forwarded initialized should decode")
        else {
            panic!("expected forwarded initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_initialize_with_expected_headers(
    expected_tenant_id: &str,
    expected_project_id: Option<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_tenant_id = expected_tenant_id.to_string();
    let expected_project_id = expected_project_id.map(str::to_string);
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-tenant-id")
                        .and_then(|value| value.to_str().ok()),
                    Some(expected_tenant_id.as_str())
                );
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-project-id")
                        .and_then(|value| value.to_str().ok()),
                    expected_project_id.as_deref()
                );
                Ok(response)
            },
        )
        .await
        .expect("websocket should accept");
        expect_remote_initialize(&mut websocket).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_hidden_server_request(
    request: JSONRPCRequest,
    expected_error_message: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_request_id = request.id.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(request.clone()))
                    .expect("server request should serialize")
                    .into(),
            ))
            .await
            .expect("server request should send");

        let frame = websocket
            .next()
            .await
            .expect("server request response should exist")
            .expect("server request response should decode");
        let Message::Text(text) = frame else {
            panic!("expected server request response text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&text).expect("server request response should decode")
        else {
            panic!("expected server request error");
        };
        assert_eq!(error.id, expected_request_id);
        assert_eq!(error.error.code, super::super::super::INVALID_PARAMS_CODE);
        assert_eq!(error.error.message, expected_error_message);

        tokio::time::sleep(Duration::from_millis(250)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_thread_start_then_server_requests()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                    let frame = websocket
                        .next()
                        .await
                        .expect("initialized frame should exist")
                        .expect("initialized frame should decode");
                    let Message::Text(text) = frame else {
                        panic!("expected initialized text frame");
                    };
                    let JSONRPCMessage::Notification(notification) =
                        serde_json::from_str(&text).expect("initialized should decode")
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");

                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "thread/start");
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "thread": {
                                        "id": "thread-recovered",
                                        "forkedFromId": null,
                                        "preview": "",
                                        "ephemeral": true,
                                        "modelProvider": "openai",
                                        "createdAt": 1,
                                        "updatedAt": 1,
                                        "status": {
                                            "type": "idle",
                                        },
                                        "path": null,
                                        "cwd": "/tmp/recovered-worker",
                                        "cliVersion": "0.0.0-test",
                                        "source": "cli",
                                        "agentNickname": null,
                                        "agentRole": null,
                                        "gitInfo": null,
                                        "name": null,
                                        "turns": [],
                                    },
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": "/tmp/recovered-worker",
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess",
                                    },
                                    "reasoningEffort": null,
                                }),
                            }))
                            .expect("thread/start response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("thread/start response should send");

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-user-input".to_string()),
                                method: "item/tool/requestUserInput".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-recovered",
                                    "turnId": "turn-recovered",
                                    "itemId": "tool-call-recovered",
                                    "questions": [{
                                        "id": "mode",
                                        "header": "Mode",
                                        "question": "Pick execution mode",
                                        "isOther": false,
                                        "isSecret": false,
                                        "options": [],
                                    }],
                                })),
                                trace: None,
                            }))
                            .expect("user-input request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("user-input request should send");

                    let JSONRPCMessage::Response(user_input_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected user-input response");
                    };
                    assert_eq!(
                        user_input_response.id,
                        RequestId::String("downstream-user-input".to_string())
                    );
                    assert_eq!(
                        user_input_response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": ["safe"],
                                },
                            },
                        })
                    );

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-refresh".to_string()),
                                method: "account/chatgptAuthTokens/refresh".to_string(),
                                params: Some(serde_json::json!({
                                    "reason": "unauthorized",
                                    "previousAccountId": "acct-recovered",
                                })),
                                trace: None,
                            }))
                            .expect("refresh request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("refresh request should send");

                    let JSONRPCMessage::Response(refresh_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected refresh response");
                    };
                    assert_eq!(
                        refresh_response.id,
                        RequestId::String("downstream-refresh".to_string())
                    );
                    assert_eq!(
                        refresh_response.result,
                        serde_json::json!({
                            "accessToken": "access-token-recovered",
                            "chatgptAccountId": "acct-recovered",
                            "chatgptPlanType": "pro",
                        })
                    );

                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_primary_login_completed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                    assert_eq!(request.method, "account/login/start");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "type": "chatgpt",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "type": "chatgpt",
                                    "loginId": "login-reconnected",
                                    "authUrl": "https://example.com/login",
                                }),
                            }))
                            .expect("login response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("login response should send");
                    send_remote_notification(
                        &mut websocket,
                        "account/login/completed",
                        serde_json::json!({
                            "loginId": "login-reconnected",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_mcp_oauth_login_completed() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                    assert_eq!(request.method, "mcpServer/oauth/login");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "name": "shared-mcp",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "authorizationUrl": "https://example.com/oauth/shared-mcp",
                                }),
                            }))
                            .expect("oauth response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("oauth response should send");
                    send_remote_notification(
                        &mut websocket,
                        "mcpServer/oauthLogin/completed",
                        serde_json::json!({
                            "name": "shared-mcp",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
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
                    let JSONRPCMessage::Notification(initialized) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(initialized.method, "initialized");
                    assert_eq!(initialized.params, None);

                    let replay_watch_request = read_websocket_request(&mut websocket).await;
                    assert_eq!(replay_watch_request.method, "fs/watch");
                    assert_eq!(
                        replay_watch_request.id,
                        RequestId::String("gateway-replay-fs-watch:watch-shared".to_string())
                    );
                    assert_json_params_eq(
                        replay_watch_request.params,
                        Some(serde_json::json!({
                            "watchId": "watch-shared",
                            "path": "/tmp/shared/project/.git/HEAD",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: replay_watch_request.id,
                                result: serde_json::json!({
                                    "path": "/tmp/shared/project/.git/HEAD",
                                }),
                            }))
                            .expect("replayed fs/watch response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("replayed fs/watch response should send");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_replay_failure() -> String
{
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

        let JSONRPCMessage::Notification(initialized) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(initialized.method, "initialized");
        assert_eq!(initialized.params, None);

        websocket
            .close(None)
            .await
            .expect("close frame should send before replayed fs/watch");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_item_delta_notification() -> String {
    start_mock_remote_server_for_notification(ServerNotification::AgentMessageDelta(
        codex_app_server_protocol::AgentMessageDeltaNotification {
            thread_id: "thread-visible".to_string(),
            turn_id: "turn-visible".to_string(),
            item_id: "item-visible".to_string(),
            delta: "streamed text".to_string(),
        },
    ))
    .await
}

pub(crate) async fn start_mock_remote_server_for_notification(
    notification: ServerNotification,
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

        let notification =
            tagged_type_to_notification(notification).expect("notification should serialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Notification(notification))
                    .expect("notification should serialize")
                    .into(),
            ))
            .await
            .expect("notification should send");

        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    format!("ws://{addr}")
}
