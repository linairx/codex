use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_limits::RATE_LIMITED_ERROR_CODE;
use crate::northbound::v2_limits::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_0_websocket_late.rs"]
mod v2_tests_cases_0_websocket_late;

#[tokio::test]
async fn websocket_upgrade_rejects_server_requests_above_pending_limit_without_closing() {
    let (rejection_observed_tx, rejection_observed_rx) = oneshot::channel();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let rejection_observed_tx = rejection_observed_tx;
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-1".to_string()),
                    method: "item/commandExecution/requestApproval".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "itemId": "item-visible-1",
                        "startedAtMs": 0,
                        "cwd": "/tmp",
                        "reason": "Need approval 1",
                        "command": "pwd",
                    })),
                    trace: None,
                }))
                .expect("first server request should serialize")
                .into(),
            ))
            .await
            .expect("first server request should send");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-2".to_string()),
                    method: "item/commandExecution/requestApproval".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "itemId": "item-visible-2",
                        "startedAtMs": 0,
                        "cwd": "/tmp",
                        "reason": "Need approval 2",
                        "command": "ls",
                    })),
                    trace: None,
                }))
                .expect("second server request should serialize")
                .into(),
            ))
            .await
            .expect("second server request should send");

        let rejected_request_id = loop {
            let Message::Text(text) = websocket
                .next()
                .await
                .expect("server request follow-up should exist")
                .expect("server request follow-up should decode")
            else {
                panic!("expected server request follow-up text frame");
            };
            match serde_json::from_str::<JSONRPCMessage>(&text)
                .expect("server request follow-up should decode")
            {
                JSONRPCMessage::Error(error) => {
                    assert_eq!(error.error.code, RATE_LIMITED_ERROR_CODE);
                    assert_eq!(
                        error.error.message,
                        TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
                    );
                    break error.id;
                }
                JSONRPCMessage::Response(response) => {
                    assert_eq!(response.result, serde_json::json!({ "approved": true }));
                }
                message => panic!("unexpected server request follow-up: {message:?}"),
            }
        };
        assert_eq!(
            rejected_request_id == RequestId::String("server-request-1".to_string())
                || rejected_request_id == RequestId::String("server-request-2".to_string()),
            true
        );
        rejection_observed_tx
            .send(())
            .expect("rejection observation should send");

        let request = loop {
            let Message::Text(text) = websocket
                .next()
                .await
                .expect("follow-up message should exist")
                .expect("follow-up message should decode")
            else {
                panic!("expected follow-up message text frame");
            };
            match serde_json::from_str::<JSONRPCMessage>(&text)
                .expect("follow-up message should decode")
            {
                JSONRPCMessage::Request(request) => break request,
                JSONRPCMessage::Response(response) => {
                    assert_eq!(response.result, serde_json::json!({ "approved": true }));
                }
                message => panic!("unexpected follow-up message: {message:?}"),
            }
        };
        assert_eq!(request.method, "model/list");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({
                        "data": [],
                        "nextCursor": null,
                    }),
                }))
                .expect("follow-up response should serialize")
                .into(),
            ))
            .await
            .expect("follow-up response should send");

        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
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
        timeouts: GatewayV2Timeouts {
            initialize: Duration::from_secs(30),
            client_send: Duration::from_secs(10),
            reconnect_retry_backoff: Duration::from_secs(1),
            max_pending_server_requests: 1,
            max_pending_client_requests: 1,
        },
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        let JSONRPCMessage::Request(first_request) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected first forwarded server request");
        };
        assert_eq!(
            first_request.id,
            RequestId::String("server-request-1".to_string())
        );
        assert_eq!(
            first_request.method,
            "item/commandExecution/requestApproval"
        );

        rejection_observed_rx
            .await
            .expect("downstream should observe pending-limit rejection");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: first_request.id.clone(),
                    result: serde_json::json!({
                        "approved": true,
                    }),
                }))
                .expect("first server request response should serialize")
                .into(),
            ))
            .await
            .expect("first server request response should send");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({
                        "cursor": null,
                        "limit": null,
                        "includeHidden": true,
                    })),
                    trace: None,
                }))
                .expect("follow-up request should serialize")
                .into(),
            ))
            .await
            .expect("follow-up request should send");

        let response = loop {
            match read_websocket_message(&mut websocket).await {
                JSONRPCMessage::Response(response)
                    if response.id == RequestId::String("model-list".to_string()) =>
                {
                    break response;
                }
                JSONRPCMessage::Notification(_) => continue,
                JSONRPCMessage::Response(response) => {
                    panic!("unexpected follow-up response id: {:?}", response.id);
                }
                JSONRPCMessage::Error(error) => {
                    panic!("unexpected follow-up error: {error:?}");
                }
                JSONRPCMessage::Request(request) => {
                    panic!("unexpected follow-up request: {request:?}");
                }
            }
        };
        assert_eq!(response.id, RequestId::String("model-list".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            })
        );
    })
    .await;

    assert!(logs.contains(
        "rejecting downstream server request because the gateway websocket connection is saturated"
    ));
    assert!(logs.contains("tenant-a"), "{logs}");
    assert!(logs.contains("project-a"), "{logs}");
    assert!(logs.contains("worker_websocket_url"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("limit=1"));
    assert!(logs.contains("request_id=String(\"server-request-2\")"));
    assert!(logs.contains("item/commandExecution/requestApproval"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains("pending_downstream_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));

    assert_v2_server_request_rejection_and_lifecycle_metrics(
        &metrics,
        "item/commandExecution/requestApproval",
        "pending_limit",
        &[
            (
                "downstream_server_request_forwarded",
                "item/commandExecution/requestApproval",
                1,
            ),
            (
                "downstream_server_request_rejected_pending_limit",
                "item/commandExecution/requestApproval",
                1,
            ),
            (
                "downstream_server_request_rejection_delivered",
                "item/commandExecution/requestApproval",
                1,
            ),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivered", "response", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_applies_scope_headers_and_rate_limits() {
    let metrics = in_memory_metrics();
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-a".to_string(),
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: Some(1),
            turn_start_quota_per_minute: None,
        }),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
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

    let mut request = format!("ws://{addr}/")
        .into_client_request()
        .expect("request should build");
    request.headers_mut().insert(
        "x-codex-tenant-id",
        "tenant-b".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-a".parse().expect("project header"),
    );
    let (mut websocket, _response) = connect_async(request)
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-read".to_string()),
                method: "thread/read".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "includeTurns": false
                })),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("thread read should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread read error response");
    };
    assert_eq!(error.id, RequestId::String("thread-read".to_string()));
    assert_eq!(error.error.code, INVALID_PARAMS_CODE);
    assert_eq!(error.error.message, "thread not found: thread-a");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("model-list".to_string()),
                method: "model/list".to_string(),
                params: Some(serde_json::json!({})),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("model list should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected model list error response");
    };
    assert_eq!(error.id, RequestId::String("model-list".to_string()));
    assert_eq!(error.error.code, RATE_LIMITED_ERROR_CODE);
    let retry_after_seconds = error
        .error
        .data
        .as_ref()
        .and_then(|data| data.get("retryAfterSeconds"))
        .and_then(serde_json::Value::as_u64)
        .expect("retryAfterSeconds should be present");
    assert_eq!((59..=60).contains(&retry_after_seconds), true);

    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("thread/read", "invalid_params", 1),
            ("model/list", "rate_limited", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_enforces_turn_start_quota() {
    let metrics = in_memory_metrics();
    let initialize_response = test_initialize_response().await;
    let turn_start_params = serde_json::json!({
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "collaborationMode": null,
        "input": [],
        "cwd": "/tmp/project",
        "effort": null,
        "model": "gpt-5",
        "outputSchema": null,
        "personality": null,
        "responsesapiClientMetadata": null,
        "sandboxPolicy": null,
        "summary": null,
        "threadId": "thread-visible",
    });
    let websocket_url =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "turn/start",
            Some(turn_start_params.clone()),
            serde_json::json!({
                "turn": {
                    "id": "turn-1",
                },
            }),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: None,
            turn_start_quota_per_minute: Some(1),
        }),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
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
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("turn-start-1".to_string()),
                method: "turn/start".to_string(),
                params: Some(turn_start_params.clone()),
                trace: None,
            }))
            .expect("first turn/start request should serialize")
            .into(),
        ))
        .await
        .expect("first turn/start should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected first turn/start response");
    };
    assert_eq!(response.id, RequestId::String("turn-start-1".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "turn": {
                "id": "turn-1",
            },
        })
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("turn-start-2".to_string()),
                method: "turn/start".to_string(),
                params: Some(turn_start_params),
                trace: None,
            }))
            .expect("second turn/start request should serialize")
            .into(),
        ))
        .await
        .expect("second turn/start should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected second turn/start error response");
    };
    assert_eq!(error.id, RequestId::String("turn-start-2".to_string()));
    assert_eq!(error.error.code, RATE_LIMITED_ERROR_CODE);
    assert_eq!(
        error.error.message,
        "turn start quota exceeded for tenant default"
    );
    let retry_after_seconds = error
        .error
        .data
        .as_ref()
        .and_then(|data| data.get("retryAfterSeconds"))
        .and_then(serde_json::Value::as_u64)
        .expect("retryAfterSeconds should be present");
    assert_eq!((59..=60).contains(&retry_after_seconds), true);

    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("turn/start", "ok", 1),
            ("turn/start", "rate_limited", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_scope_headers_to_remote_worker_session() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize_with_expected_headers(
        "tenant-visible",
        Some("project-visible"),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
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

    let mut request = format!("ws://{addr}/")
        .into_client_request()
        .expect("request should build");
    request.headers_mut().insert(
        "x-codex-tenant-id",
        "tenant-visible".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-visible".parse().expect("project header"),
    );
    let (mut websocket, _response) = connect_async(request)
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_thread_resume_unknown_path_over_jsonrpc() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
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
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-resume-path".to_string()),
                method: "thread/resume".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            }))
            .expect("thread/resume path request should serialize")
            .into(),
        ))
        .await
        .expect("thread/resume path request should send");

    let JSONRPCMessage::Error(path_error) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread/resume path error response");
    };
    assert_eq!(
        path_error.id,
        RequestId::String("thread-resume-path".to_string())
    );
    assert_eq!(path_error.error.code, INVALID_PARAMS_CODE);
    assert_eq!(
        path_error.error.message,
        "thread not found: /tmp/rollout.jsonl"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("thread/resume", "invalid_params", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_thread_fork_unknown_path_over_jsonrpc() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
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
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-fork-path".to_string()),
                method: "thread/fork".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            }))
            .expect("thread/fork path request should serialize")
            .into(),
        ))
        .await
        .expect("thread/fork path request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread/fork path error response");
    };
    assert_eq!(error.id, RequestId::String("thread-fork-path".to_string()));
    assert_eq!(error.error.code, INVALID_PARAMS_CODE);
    assert_eq!(error.error.message, "thread not found: /tmp/rollout.jsonl");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("thread/fork", "invalid_params", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}
