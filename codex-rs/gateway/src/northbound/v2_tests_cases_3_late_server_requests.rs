use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_chatgpt_auth_tokens_refresh_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("refresh-request-1".to_string()),
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    params: Some(serde_json::json!({
                        "reason": "unauthorized",
                        "previousAccountId": "acct-123",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("refresh response should exist")
            .expect("refresh response should decode")
        else {
            panic!("expected refresh response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("refresh response should decode")
        else {
            panic!("expected refresh response");
        };
        assert_eq!(
            response.id,
            RequestId::String("refresh-request-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "accessToken": "access-token-1",
                "chatgptAccountId": "acct-123",
                "chatgptPlanType": "pro",
            })
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded refresh request");
    };
    assert_eq!(
        request.id,
        RequestId::String("refresh-request-1".to_string())
    );
    assert_eq!(request.method, "account/chatgptAuthTokens/refresh");
    assert_json_params_eq(
        request.params,
        Some(serde_json::json!({
            "reason": "unauthorized",
            "previousAccountId": "acct-123",
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "accessToken": "access-token-1",
                    "chatgptAccountId": "acct-123",
                    "chatgptPlanType": "pro",
                }),
            }))
            .expect("refresh response should serialize")
            .into(),
        ))
        .await
        .expect("refresh response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_exec_command_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-exec-approval-1".to_string()),
                    method: "execCommandApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "approvalId": "approval-visible",
                        "command": ["echo", "hello"],
                        "cwd": "/tmp/workspace",
                        "reason": "Need to run a visible command",
                        "parsedCmd": [{
                            "type": "unknown",
                            "cmd": "echo hello",
                        }],
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-exec-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ExecCommandApprovalResponse {
                decision: ReviewDecision::Approved,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-exec-approval-1".to_string()),
                result: serde_json::to_value(ExecCommandApprovalResponse {
                    decision: ReviewDecision::Approved,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_apply_patch_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-patch-approval-1".to_string()),
                    method: "applyPatchApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "fileChanges": {
                            "README.md": {
                                "changeType": "added",
                                "oldContent": null,
                                "newContent": "hello\\n",
                            },
                        },
                        "reason": "Need to write visible changes",
                        "grantRoot": "/tmp/workspace",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-patch-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ApplyPatchApprovalResponse {
                decision: ReviewDecision::ApprovedForSession,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-patch-approval-1".to_string()),
                result: serde_json::to_value(ApplyPatchApprovalResponse {
                    decision: ReviewDecision::ApprovedForSession,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fails_closed_when_server_request_answer_targets_exhausted_account() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (downstream_answer_tx, downstream_answer_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let thread_start_request = read_websocket_request(&mut websocket).await;
        assert_eq!(thread_start_request.method, "thread/start");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: thread_start_request.id,
                    result: serde_json::json!({
                        "thread": {
                            "id": "thread-worker-a",
                            "forkedFromId": null,
                            "preview": "/tmp/worker-a",
                            "ephemeral": true,
                            "modelProvider": "openai",
                            "createdAt": 1,
                            "updatedAt": 1,
                            "status": {
                                "type": "idle",
                            },
                            "path": null,
                            "cwd": "/tmp/worker-a",
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
                        "cwd": "/tmp/worker-a",
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
                    id: RequestId::String("srv-user-input".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-worker-a",
                        "turnId": "turn-worker-a",
                        "itemId": "tool-call-worker-a",
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
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let maybe_response = timeout(Duration::from_millis(300), websocket.next())
            .await
            .ok()
            .and_then(|frame| frame)
            .and_then(Result::ok)
            .and_then(|message| match message {
                Message::Text(text) => Some(text.to_string()),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
                Message::Close(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => None,
            });
        downstream_answer_tx
            .send(maybe_response)
            .expect("downstream answer observation should send");
    });

    let worker_b = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            format!("ws://{downstream_addr}"),
            Some("acct-a".to_string()),
        ),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(
            GatewayV2SessionFactory::remote_multi_with_account_ids(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: format!("ws://{downstream_addr}"),
                            auth_token: None,
                        },
                        client_name: "codex-gateway".to_string(),
                        client_version: "0.0.0-test".to_string(),
                        experimental_api: false,
                        mcp_server_openai_form_elicitation: false,
                        opt_out_notification_methods: Vec::new(),
                        channel_capacity: 4,
                    },
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                        client_name: "codex-gateway".to_string(),
                        client_version: "0.0.0-test".to_string(),
                        experimental_api: false,
                        mcp_server_openai_form_elicitation: false,
                        opt_out_notification_methods: Vec::new(),
                        channel_capacity: 4,
                    },
                ],
                test_initialize_response().await,
                vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
            )
            .with_worker_health(worker_health.clone()),
        )),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    send_initialized(&mut websocket).await;
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-start".to_string()),
                method: "thread/start".to_string(),
                params: Some(serde_json::json!({
                    "model": null,
                    "modelProvider": null,
                    "serviceTier": null,
                    "cwd": "/tmp/worker-a",
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "sandbox": null,
                    "config": null,
                    "serviceName": null,
                    "baseInstructions": null,
                    "developerInstructions": null,
                    "personality": null,
                    "ephemeral": true,
                    "sessionStartSource": null,
                    "dynamicTools": null,
                    "experimentalRawEvents": false,
                    "persistExtendedHistory": false,
                })),
                trace: None,
            }))
            .expect("thread/start request should serialize")
            .into(),
        ))
        .await
        .expect("thread/start request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread/start response");
    };
    assert_eq!(response.id, RequestId::String("thread-start".to_string()));

    let JSONRPCMessage::Request(user_input_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded user-input request");
    };
    assert_eq!(user_input_request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        user_input_request.params,
        Some(serde_json::json!({
            "threadId": "thread-worker-a",
            "turnId": "turn-worker-a",
            "itemId": "tool-call-worker-a",
            "questions": [{
                "id": "mode",
                "header": "Mode",
                "question": "Pick execution mode",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    worker_health.mark_account_exhausted_for_worker(0, "quota reached".to_string());
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: user_input_request.id,
                result: serde_json::json!({
                    "answers": {
                        "mode": {
                            "answers": ["safe"],
                        },
                    },
                }),
            }))
            .expect("user-input response should serialize")
            .into(),
        ))
        .await
        .expect("user-input response should send");

    match timeout(Duration::from_secs(2), websocket.next())
        .await
        .expect("websocket should close or reset")
    {
        Some(Ok(Message::Close(Some(close_frame)))) => {
            assert_eq!(u16::from(close_frame.code), close_code::ERROR);
            assert!(
                    close_frame.reason.contains(
                        "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
                    ),
                    "{}",
                    close_frame.reason
                );
        }
        Some(Ok(Message::Close(None))) | None => {}
        Some(Err(err)) => {
            assert!(
                err.to_string().contains("reset without closing handshake"),
                "{err}"
            );
        }
        Some(Ok(message)) => panic!("expected websocket close or reset, got {message:?}"),
    }
    assert_eq!(
        downstream_answer_rx
            .await
            .expect("downstream answer observation should arrive"),
        None
    );
    assert_v2_server_request_answer_account_exhaustion_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "response", 1),
            (
                "downstream_server_request_forwarded",
                "item/tool/requestUserInput",
                1,
            ),
        ],
        &[("response", 1)],
        &[(0, "active_thread_handoff_failure", 1)],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-a"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "default",
            "projectId": null,
            "method": "serverRequest/respond",
            "threadId": "thread-worker-a",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
        })
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("active_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("active_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("default")
    );
    assert_eq!(health.last_account_capacity_event_project_id, None);
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);

    server_task.abort();
    let _ = server_task.await;
}
