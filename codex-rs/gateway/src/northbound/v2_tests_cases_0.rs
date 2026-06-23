use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_0_initialize.rs"]
mod v2_tests_cases_0_initialize;

#[path = "v2_tests_cases_0_websocket.rs"]
mod v2_tests_cases_0_websocket;

pub(crate) use super::v2_tests_cases_0_late::*;

#[tokio::test]
async fn await_io_with_timeout_returns_result_before_timeout() {
    let result = await_io_with_timeout(
        async { Ok::<_, std::io::Error>("ok") },
        Duration::from_millis(50),
        "timed out",
    )
    .await
    .expect("future should complete");

    assert_eq!(result, "ok");
}

#[tokio::test]
async fn await_io_with_timeout_returns_timed_out_error() {
    let error = await_io_with_timeout(
        async {
            sleep(Duration::from_millis(50)).await;
            Ok::<_, std::io::Error>(())
        },
        Duration::from_millis(1),
        "timed out",
    )
    .await
    .expect_err("future should time out");

    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(error.to_string(), "timed out");
}

#[tokio::test]
async fn websocket_upgrade_forwards_server_requests_received_during_downstream_initialize() {
    let initialize_response = test_initialize_response().await;
    let (websocket_url, response_rx) =
        start_mock_remote_server_that_sends_server_request_during_initialize().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-init".to_string(),
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected server request forwarded after initialize");
    };
    assert_eq!(request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        request.params,
        Some(serde_json::json!({
            "threadId": "thread-init",
            "turnId": "turn-init",
            "itemId": "item-init",
            "questions": [{
                "id": "question-init",
                "header": "Mode",
                "question": "Pick one",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({ "answer": "accepted" }),
            }))
            .expect("response should serialize")
            .into(),
        ))
        .await
        .expect("server request response should send");
    let response = response_rx
        .await
        .expect("mock remote should report server request response");
    assert_eq!(response.id, RequestId::String("srv-init".to_string()));
    assert_eq!(response.result, serde_json::json!({ "answer": "accepted" }));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_unknown_server_requests_received_during_downstream_initialize() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_that_sends_unknown_server_request_during_initialize().await;
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
    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("model-list".to_string()),
        "model/list",
        serde_json::json!({}),
    )
    .await;

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected model/list response after rejected setup-time request");
    };
    assert_eq!(response.id, RequestId::String("model-list".to_string()));
    assert_eq!(response.result, serde_json::json!({ "models": [] }));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_reports_downstream_binary_initialize_frame_as_protocol_violation() {
    let websocket_url = start_mock_remote_server_that_sends_binary_during_initialize().await;
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("initialize request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize error response");
    };
    assert_eq!(error.id, RequestId::String("initialize".to_string()));
    assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
    assert!(
        error
            .error
            .message
            .contains("sent non-text initialize frame")
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "downstream",
        "invalid_binary",
        "downstream_protocol_violation",
        "initialize",
        "downstream_protocol_violation",
        1,
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_reports_downstream_invalid_initialize_response_as_protocol_violation() {
    let websocket_url =
        start_mock_remote_server_that_sends_invalid_jsonrpc_during_initialize().await;
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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
        "tenant-downstream-protocol".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-downstream-protocol"
            .parse()
            .expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("initialize".to_string()),
                    method: "initialize".to_string(),
                    params: Some(
                        serde_json::to_value(InitializeParams {
                            client_info: ClientInfo {
                                name: "codex-tui".to_string(),
                                title: None,
                                version: "0.0.0-test".to_string(),
                            },
                            capabilities: None,
                        })
                        .expect("initialize params should serialize"),
                    ),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("initialize request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected initialize error response");
        };
        assert_eq!(error.id, RequestId::String("initialize".to_string()));
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert!(
            error
                .error
                .message
                .contains("sent invalid initialize response")
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"initialize\""), "{logs}");
    assert!(
        logs.contains("outcome=\"downstream_protocol_violation\""),
        "{logs}"
    );
    assert!(
        logs.contains("tenant_id=\"tenant-downstream-protocol\""),
        "{logs}"
    );
    assert!(
        logs.contains("project_id=\"project-downstream-protocol\""),
        "{logs}"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "downstream",
        "invalid_jsonrpc",
        "downstream_protocol_violation",
        "initialize",
        "downstream_protocol_violation",
        1,
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_reports_downstream_wrong_id_initialize_response_as_protocol_violation() {
    let websocket_url =
        start_mock_remote_server_that_sends_wrong_id_response_during_initialize().await;
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("initialize request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize error response");
    };
    assert_eq!(error.id, RequestId::String("initialize".to_string()));
    assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
    assert!(
        error
            .error
            .message
            .contains("sent invalid initialize response")
    );
    assert!(error.error.message.contains("not-initialize"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "downstream",
        "invalid_jsonrpc",
        "downstream_protocol_violation",
        "initialize",
        "downstream_protocol_violation",
        1,
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_reports_downstream_wrong_id_initialize_error_as_protocol_violation() {
    let websocket_url =
        start_mock_remote_server_that_sends_wrong_id_error_during_initialize().await;
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("initialize request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize error response");
    };
    assert_eq!(error.id, RequestId::String("initialize".to_string()));
    assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
    assert!(
        error
            .error
            .message
            .contains("sent invalid initialize response")
    );
    assert!(error.error.message.contains("not-initialize"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "downstream",
        "invalid_jsonrpc",
        "downstream_protocol_violation",
        "initialize",
        "downstream_protocol_violation",
        1,
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_initialize_capabilities_to_all_multi_worker_sessions() {
    let recorded_initialize_params = Arc::new(Mutex::new(Vec::new()));
    let worker_a =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let worker_b =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 8,
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
                    channel_capacity: 8,
                },
            ],
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize_with_capabilities(
        &mut websocket,
        Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec![
                "thread/started".to_string(),
                "item/agentMessage/delta".to_string(),
            ]),
        }),
    )
    .await;

    let expected_initialize = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec![
                "thread/started".to_string(),
                "item/agentMessage/delta".to_string(),
            ]),
        }),
    };
    timeout(Duration::from_secs(5), async {
        loop {
            let ready = {
                let recorded = recorded_initialize_params.lock().await;
                recorded.len() == 2
            };
            if ready {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both workers should receive initialize params");
    assert_eq!(
        *recorded_initialize_params.lock().await,
        vec![expected_initialize.clone(), expected_initialize]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_origin_header() {
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
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
        "origin",
        "https://example.com".parse().expect("origin header"),
    );
    let err = connect_async(request)
        .await
        .expect_err("websocket handshake should fail");
    let response = match err {
        WebSocketError::Http(response) => response,
        other => panic!("expected HTTP handshake error, got {other:?}"),
    };

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_requires_bearer_token_when_configured() {
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::BearerToken {
            token: "secret-token".to_string(),
        },
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
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

    let request = format!("ws://{addr}/")
        .into_client_request()
        .expect("request should build");
    let err = connect_async(request)
        .await
        .expect_err("websocket handshake should fail");
    let response = match err {
        WebSocketError::Http(response) => response,
        other => panic!("expected HTTP handshake error, got {other:?}"),
    };

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_returns_not_implemented_without_v2_runtime() {
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: None,
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let request = format!("ws://{addr}/")
        .into_client_request()
        .expect("request should build");
    let err = connect_async(request)
        .await
        .expect_err("websocket handshake should fail");
    let response = match err {
        WebSocketError::Http(response) => response,
        other => panic!("expected HTTP handshake error, got {other:?}"),
    };

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_initialize_times_out() {
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
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
        timeouts: GatewayV2Timeouts {
            initialize: Duration::from_millis(50),
            client_send: Duration::from_secs(10),
            reconnect_retry_backoff: Duration::from_secs(1),
            max_pending_server_requests:
                super::super::super::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
            max_pending_client_requests:
                super::super::super::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
        },
    })
    .await;

    let mut request = format!("ws://{addr}/")
        .into_client_request()
        .expect("request should build");
    request.headers_mut().insert(
        "x-codex-tenant-id",
        "tenant-timeout".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-timeout".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        let frame = timeout(Duration::from_secs(2), websocket.next())
            .await
            .expect("close frame should arrive")
            .expect("websocket should yield frame")
            .expect("close frame should decode");
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::POLICY
        );
        assert_eq!(
            close_frame.reason,
            super::super::super::INITIALIZE_TIMEOUT_CLOSE_REASON
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"initialize\""), "{logs}");
    assert!(logs.contains("outcome=\"timed_out\""), "{logs}");
    assert!(logs.contains("tenant_id=\"tenant-timeout\""), "{logs}");
    assert!(logs.contains("project_id=\"project-timeout\""), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_request_metrics(&metrics, &[("initialize", "timed_out", 1)]);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_pre_initialize_text_is_not_jsonrpc() {
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
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
        "tenant-invalid-payload".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-invalid-payload".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::PROTOCOL
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"invalid_client_payload\""),
        "{logs}"
    );
    assert!(
        logs.contains(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON),
        "{logs}"
    );
    assert!(
        logs.contains("tenant_id=\"tenant-invalid-payload\""),
        "{logs}"
    );
    assert!(
        logs.contains("project_id=\"project-invalid-payload\""),
        "{logs}"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "pre_initialize",
        "invalid_jsonrpc",
        "invalid_client_payload",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_post_initialize_text_is_not_jsonrpc() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
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
        "tenant-post-text".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-post-text".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::PROTOCOL
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"invalid_client_payload\""),
        "{logs}"
    );
    assert!(
        logs.contains(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"tenant-post-text\""), "{logs}");
    assert!(logs.contains("project_id=\"project-post-text\""), "{logs}");
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "post_initialize",
        "invalid_jsonrpc",
        "invalid_client_payload",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_pre_initialize_binary_is_not_utf8() {
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
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
        "tenant-invalid-binary".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-invalid-binary".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        websocket
            .send(Message::Binary(vec![0xff, 0xfe, 0xfd].into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::INVALID
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::super::super::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"invalid_client_payload\""),
        "{logs}"
    );
    assert!(
        logs.contains(super::super::super::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON),
        "{logs}"
    );
    assert!(
        logs.contains("tenant_id=\"tenant-invalid-binary\""),
        "{logs}"
    );
    assert!(
        logs.contains("project_id=\"project-invalid-binary\""),
        "{logs}"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "pre_initialize",
        "invalid_utf8",
        "invalid_client_payload",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_post_initialize_binary_is_not_utf8() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
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
        "tenant-post-binary".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-post-binary".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Binary(vec![0xff, 0xfe, 0xfd].into()))
            .await
            .expect("invalid binary payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::INVALID
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::super::super::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"invalid_client_payload\""),
        "{logs}"
    );
    assert!(
        logs.contains(super::super::super::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"tenant-post-binary\""), "{logs}");
    assert!(
        logs.contains("project_id=\"project-post-binary\""),
        "{logs}"
    );
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "post_initialize",
        "invalid_utf8",
        "invalid_client_payload",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_ignores_non_request_jsonrpc_messages_before_initialize() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
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

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("notification should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize notification should send");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("unexpected-response".to_string()),
                result: serde_json::json!({}),
            }))
            .expect("response should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize response should send");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                id: RequestId::String("unexpected-error".to_string()),
                error: JSONRPCErrorError {
                    code: super::super::super::INVALID_REQUEST_CODE,
                    message: "unexpected error".to_string(),
                    data: None,
                },
            }))
            .expect("error should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize error should send");

    assert!(
        timeout(Duration::from_millis(200), websocket.next())
            .await
            .is_err(),
        "gateway should ignore pre-initialize notification/response/error frames"
    );

    send_initialize(&mut websocket).await;

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_ignores_non_request_jsonrpc_binary_messages_before_initialize() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
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

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    websocket
        .send(Message::Binary(
            serde_json::to_vec(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("notification should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize notification should send");

    websocket
        .send(Message::Binary(
            serde_json::to_vec(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("unexpected-response".to_string()),
                result: serde_json::json!({}),
            }))
            .expect("response should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize response should send");

    websocket
        .send(Message::Binary(
            serde_json::to_vec(&JSONRPCMessage::Error(JSONRPCError {
                id: RequestId::String("unexpected-error".to_string()),
                error: JSONRPCErrorError {
                    code: super::super::super::INVALID_REQUEST_CODE,
                    message: "unexpected error".to_string(),
                    data: None,
                },
            }))
            .expect("error should serialize")
            .into(),
        ))
        .await
        .expect("pre-initialize error should send");

    assert!(
        timeout(Duration::from_millis(200), websocket.next())
            .await
            .is_err(),
        "gateway should ignore pre-initialize notification/response/error binary frames"
    );

    send_initialize(&mut websocket).await;

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_initialized_notification_to_multi_worker_sessions() {
    let worker_a = start_mock_remote_server_expecting_forwarded_initialized().await;
    let worker_b = start_mock_remote_server_expecting_forwarded_initialized().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
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
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("initialized notification should serialize")
            .into(),
        ))
        .await
        .expect("initialized notification should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_chatgpt_auth_tokens_login_start_to_multi_worker_sessions() {
    let (worker_a_request_tx, worker_a_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let (worker_b_request_tx, worker_b_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_request_tx,
        serde_json::json!({
            "type": "chatgptAuthTokens",
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_request_tx,
        serde_json::json!({
            "type": "chatgptAuthTokens",
        }),
    )
    .await;

    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
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
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "chatgptAuthTokens",
                    "accessToken": "access-token-1",
                    "chatgptAccountId": "acct-123",
                    "chatgptPlanType": "pro",
                })),
                trace: None,
            }))
            .expect("login request should serialize")
            .into(),
        ))
        .await
        .expect("login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({ "type": "chatgptAuthTokens" })
    );

    let worker_a_request = worker_a_request_rx
        .await
        .expect("worker A should receive login request");
    let worker_b_request = worker_b_request_rx
        .await
        .expect("worker B should receive login request");
    assert_eq!(worker_a_request.method, "account/login/start");
    assert_eq!(worker_b_request.method, "account/login/start");
    assert_eq!(worker_a_request.params, worker_b_request.params);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_api_key_login_start_to_multi_worker_sessions() {
    let (worker_a_request_tx, worker_a_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let (worker_b_request_tx, worker_b_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_request_tx,
        serde_json::json!({
            "type": "apiKey",
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_request_tx,
        serde_json::json!({
            "type": "apiKey",
        }),
    )
    .await;

    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
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
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "apiKey",
                    "apiKey": "sk-test-123",
                })),
                trace: None,
            }))
            .expect("login request should serialize")
            .into(),
        ))
        .await
        .expect("login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(response.result, serde_json::json!({ "type": "apiKey" }));

    let worker_a_request = worker_a_request_rx
        .await
        .expect("worker A should receive login request");
    let worker_b_request = worker_b_request_rx
        .await
        .expect("worker B should receive login request");
    assert_eq!(worker_a_request.method, "account/login/start");
    assert_eq!(worker_b_request.method, "account/login/start");
    assert_eq!(worker_a_request.params, worker_b_request.params);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_responds_to_ping_before_and_after_initialize() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
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

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    websocket
        .send(Message::Ping(vec![1, 2, 3].into()))
        .await
        .expect("pre-initialize ping should send");
    let Message::Pong(payload) = timeout(Duration::from_secs(2), websocket.next())
        .await
        .expect("pre-initialize pong should arrive")
        .expect("pre-initialize pong frame should exist")
        .expect("pre-initialize pong should decode")
    else {
        panic!("expected pre-initialize pong frame");
    };
    assert_eq!(payload.as_ref(), &[1, 2, 3]);

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Ping(vec![4, 5, 6].into()))
        .await
        .expect("post-initialize ping should send");
    let Message::Pong(payload) = timeout(Duration::from_secs(2), websocket.next())
        .await
        .expect("post-initialize pong should arrive")
        .expect("post-initialize pong frame should exist")
        .expect("post-initialize pong should decode")
    else {
        panic!("expected post-initialize pong frame");
    };
    assert_eq!(payload.as_ref(), &[4, 5, 6]);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_repeated_initialize_after_handshake() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
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
        "tenant-repeat".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-repeat".parse().expect("project header"),
    );
    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("initialize-again".to_string()),
                    method: "initialize".to_string(),
                    params: Some(
                        serde_json::to_value(InitializeParams {
                            client_info: ClientInfo {
                                name: "codex-tui".to_string(),
                                title: None,
                                version: "0.0.0-test".to_string(),
                            },
                            capabilities: None,
                        })
                        .expect("initialize params should serialize"),
                    ),
                    trace: None,
                }))
                .expect("initialize request should serialize")
                .into(),
            ))
            .await
            .expect("repeated initialize should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected repeated initialize error response");
        };
        assert_eq!(error.id, RequestId::String("initialize-again".to_string()));
        assert_eq!(error.error.code, super::super::super::INVALID_REQUEST_CODE);
        assert_eq!(error.error.message, "connection is already initialized");
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"initialize\""), "{logs}");
    assert!(logs.contains("outcome=\"invalid_request\""), "{logs}");
    assert!(logs.contains("tenant_id=\"tenant-repeat\""), "{logs}");
    assert!(logs.contains("project_id=\"project-repeat\""), "{logs}");
    assert_repeated_initialize_metrics(&metrics);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_repeated_initialize_binary_after_handshake() {
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
        .send(Message::Binary(
            serde_json::to_vec(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("initialize-again".to_string()),
                method: "initialize".to_string(),
                params: Some(
                    serde_json::to_value(InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    })
                    .expect("initialize params should serialize"),
                ),
                trace: None,
            }))
            .expect("initialize request should serialize")
            .into(),
        ))
        .await
        .expect("repeated initialize should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected repeated initialize error response");
    };
    assert_eq!(error.id, RequestId::String("initialize-again".to_string()));
    assert_eq!(error.error.code, super::super::super::INVALID_REQUEST_CODE);
    assert_eq!(error.error.message, "connection is already initialized");
    assert_repeated_initialize_metrics(&metrics);

    server_task.abort();
    let _ = server_task.await;
}
