use super::*;
use pretty_assertions::assert_eq;

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
async fn initialize_returns_jsonrpc_error_for_invalid_params() {
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
        "tenant-visible".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-visible".parse().expect("project header"),
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
                    params: Some(serde_json::json!({
                        "clientInfo": {
                            "name": 1
                        }
                    })),
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
        assert_eq!(error.error.code, super::super::super::INVALID_REQUEST_CODE);
        assert_eq!(
            error
                .error
                .message
                .contains("invalid request params for `initialize`"),
            true
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"initialize\""), "{logs}");
    assert!(logs.contains("outcome=\"invalid_request\""), "{logs}");
    assert!(logs.contains("tenant_id=\"tenant-visible\""), "{logs}");
    assert!(logs.contains("project_id=\"project-visible\""), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_request_metrics(&metrics, &[("initialize", "invalid_request", 1)]);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_must_be_the_first_request_for_binary_frames_too() {
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
    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    websocket
        .send(Message::Binary(
            serde_json::to_vec(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("model-list".to_string()),
                method: "model/list".to_string(),
                params: Some(serde_json::json!({})),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("binary request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected pre-initialize error response");
    };
    assert_eq!(error.id, RequestId::String("model-list".to_string()));
    assert_eq!(error.error.code, super::super::super::INVALID_REQUEST_CODE);
    assert_eq!(error.error.message, "initialize must be the first request");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_request_metrics(
        &metrics,
        "pre_initialize",
        "initialize_order",
        "model/list",
        "invalid_request",
    );

    send_initialize_with_capabilities(
        &mut websocket,
        Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: None,
        }),
    )
    .await;

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn initialize_must_be_the_first_request_for_text_frames_too() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let metrics_for_server = metrics.clone();
    {
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics_for_server), true),
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
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-preinit".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-preinit".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

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
            .expect("text request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected pre-initialize error response");
        };
        assert_eq!(error.id, RequestId::String("model-list".to_string()));
        assert_eq!(error.error.code, super::super::super::INVALID_REQUEST_CODE);
        assert_eq!(error.error.message, "initialize must be the first request");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;

        server_task.abort();
        let _ = server_task.await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_request_metrics(
        &metrics,
        "pre_initialize",
        "initialize_order",
        "model/list",
        "invalid_request",
    );
}

#[tokio::test]
async fn initialize_returns_jsonrpc_error_when_downstream_connect_fails() {
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
        "tenant-downstream".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-downstream".parse().expect("project header"),
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
        assert_eq!(
            error
                .error
                .message
                .contains("gateway failed to connect downstream app-server"),
            true
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"initialize\""), "{logs}");
    assert!(
        logs.contains("outcome=\"downstream_connect_error\""),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"tenant-downstream\""), "{logs}");
    assert!(logs.contains("project_id=\"project-downstream\""), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_request_metrics(&metrics, &[("initialize", "downstream_connect_error", 1)]);

    server_task.abort();
    let _ = server_task.await;
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

#[test]
fn pending_client_response_settlement_removes_active_request_before_delivery() {
    let (tx, _rx) = mpsc::channel(1);
    let request_id = RequestId::String("command-exec-1".to_string());
    let mut pending_client_responses = super::super::super::PendingClientResponses {
        tx,
        tasks: Vec::new(),
        count: 1,
        active: HashMap::from([(
            request_id.clone(),
            super::super::super::PendingClientRequestRoute {
                method: "command/exec".to_string(),
                request_context: GatewayRequestContext {
                    tenant_id: "tenant-visible".to_string(),
                    project_id: Some("project-visible".to_string()),
                },
                worker_id: Some(0),
                worker_websocket_url: "ws://worker-a.invalid".to_string(),
                started_at: Instant::now(),
            },
        )]),
    };

    pending_client_responses.settle_response(&request_id);

    assert_eq!(pending_client_responses.count, 0);
    assert!(pending_client_responses.active.is_empty());
}

#[tokio::test]
async fn deliver_client_server_request_answer_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Response(serde_json::json!({
                    "approved": true,
                })),
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains("failed to deliver answered server request back to downstream worker"),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"default\""), "{logs}");
    assert!(logs.contains("response_kind=\"response\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(logs.contains("thread_id=\"thread-visible\""), "{logs}");
    assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "response", 1),
        ],
        &[("response", 1)],
    );
}

#[tokio::test]
async fn deliver_client_server_request_answer_fails_closed_when_account_is_exhausted() {
    let (client_a, request_handle_a) = start_test_request_handle().await;
    let (client_b, request_handle_b) = start_test_request_handle().await;
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://worker-a.invalid".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://worker-b.invalid".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());
    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://worker-a.invalid".to_string(),
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
                    websocket_url: "ws://worker-b.invalid".to_string(),
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
    .with_worker_health(worker_health);
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: vec![
            DownstreamWorkerHandle {
                worker_id: Some(0),
                worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
                request_handle: request_handle_a,
            },
            DownstreamWorkerHandle {
                worker_id: Some(1),
                worker_websocket_url: Some("ws://worker-b.invalid".to_string()),
                request_handle: request_handle_b,
            },
        ],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory,
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: request_context.clone(),
            retry_backoff: Duration::from_secs(1),
        }),
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-worker-b".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Response(serde_json::json!({
                    "approved": true,
                })),
            )
            .await
            .expect_err("exhausted owner account should fail closed"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert_eq!(
        err.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
    );
    assert!(logs.contains("active_thread_handoff_failure"), "{logs}");
    assert!(logs.contains("response_kind=\"response\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-worker-b\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("exhausted_account_id=\"acct-b\""), "{logs}");
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "serverRequest/respond",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond",
        })
    );

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-error-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String(
                        "downstream-error-request".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-worker-b".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Error(JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                }),
            )
            .await
            .expect_err("exhausted owner account should fail closed for error replies"),
        );
    })
    .await;
    let err = err.expect("error reply delivery should fail");

    assert_eq!(
        err.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
    );
    assert!(logs.contains("active_thread_handoff_failure"), "{logs}");
    assert!(logs.contains("response_kind=\"error\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-worker-b\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("exhausted_account_id=\"acct-b\""), "{logs}");
    assert_v2_server_request_answer_account_exhaustion_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "error", 1),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "error", 1),
            ("client_server_request_delivery_failed", "response", 1),
        ],
        &[("error", 1), ("response", 1)],
        &[(1, "active_thread_handoff_failure", 2)],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("active_thread_handoff_failure".to_string(), 2)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("active_thread_handoff_failure".to_string(), 2)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published for error reply");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));

    client_a
        .shutdown()
        .await
        .expect("client A should shut down");
    client_b
        .shutdown()
        .await
        .expect("client B should shut down");
}

#[tokio::test]
async fn deliver_client_server_request_error_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Error(JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                }),
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains("failed to deliver answered server request back to downstream worker"),
        "{logs}"
    );
    assert!(logs.contains("response_kind=\"error\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(logs.contains("thread_id=\"thread-visible\""), "{logs}");
    assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "error", 1),
            ("client_server_request_delivery_failed", "error", 1),
        ],
        &[("error", 1)],
    );
}

#[tokio::test]
async fn reject_downstream_server_request_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-visible".to_string(),
        project_id: Some("project-visible".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        let gateway_request_id = RequestId::String("gateway-request".to_string());
        err = Some(
            super::super::super::reject_downstream_server_request_at_gateway_boundary(
                &downstream,
                &connection,
                super::super::super::GatewayRejectedServerRequest {
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid",
                    gateway_request_id: &gateway_request_id,
                    method: "item/tool/requestUserInput",
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                },
                JSONRPCErrorError {
                    code: super::super::super::INTERNAL_ERROR_CODE,
                    message: "gateway rejected prompt".to_string(),
                    data: None,
                },
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains(
            "failed to deliver gateway rejected server request back to downstream worker"
        ),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"tenant-visible\""), "{logs}");
    assert!(logs.contains("project_id=\"project-visible\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("method=\"item/tool/requestUserInput\""),
        "{logs}"
    );
    assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
        &metrics,
        &[(
            "downstream_server_request_rejection_delivery_failed",
            "item/tool/requestUserInput",
            1,
        )],
        "item/tool/requestUserInput",
        1,
    );
}

#[test]
fn server_request_backlog_worker_counts_groups_pending_and_answered_routes() {
    let pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-pending-a".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(2),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-a".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-b".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-b".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-c".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: None,
                worker_websocket_url: "embedded".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-c".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
    ]);
    let resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-a".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-a".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-b".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-b".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
    ]);

    assert_eq!(
        super::super::super::server_request_backlog_worker_counts(
            &pending_server_requests,
            &resolved_server_requests
        ),
        vec![
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: None,
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(1),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(2),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 3,
            },
        ]
    );
}

#[test]
fn server_request_backlog_method_counts_groups_prompt_families() {
    let pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-pending-user-input".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(2),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-user-input".to_string(),
                ),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-elicitation".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-elicitation".to_string(),
                ),
                method: "mcpServer/elicitation/request".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-permissions".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: None,
                worker_websocket_url: "embedded".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-permissions".to_string(),
                ),
                method: "item/permissions/requestApproval".to_string(),
                thread_id: Some("thread-c".to_string()),
            },
        ),
    ]);
    let resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-user-input".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-user-input".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(1),
                request_id: RequestId::String("downstream-resolved-elicitation".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-elicitation".to_string()),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                method: "mcpServer/elicitation/request".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(0),
                request_id: RequestId::String("downstream-resolved-refresh".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-refresh".to_string()),
                worker_websocket_url: "ws://worker-a.invalid".to_string(),
                method: "account/chatgptAuthTokens/refresh".to_string(),
                thread_id: None,
            },
        ),
    ]);

    assert_eq!(
        super::super::super::server_request_backlog_method_counts(
            &pending_server_requests,
            &resolved_server_requests
        ),
        vec![
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            },
        ]
    );
}

#[test]
fn server_request_resolved_notification_drops_duplicate_multi_worker_replays() {
    let gateway_request_id = RequestId::String("gateway-request-1".to_string());
    let downstream_request_id = RequestId::String("worker-request-1".to_string());
    let worker_id = Some(7);
    let mut resolved_server_requests = HashMap::from([(
        super::super::super::DownstreamServerRequestKey {
            worker_id,
            request_id: downstream_request_id.clone(),
        },
        super::super::super::ResolvedServerRequestRoute {
            gateway_request_id: gateway_request_id.clone(),
            worker_websocket_url: test_worker_websocket_url(worker_id),
            method: "item/tool/requestUserInput".to_string(),
            thread_id: Some("thread-worker-1".to_string()),
        },
    )]);
    let notification =
        ServerNotification::ServerRequestResolved(ServerRequestResolvedNotification {
            thread_id: "thread-worker-1".to_string(),
            request_id: downstream_request_id,
        });
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-visible".to_string(),
        project_id: Some("project-visible".to_string()),
    };
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);

    let translated = super::super::super::server_notification_to_jsonrpc(
        notification.clone(),
        &request_context,
        &observability,
        worker_id,
        "ws://worker-b.invalid",
        &mut resolved_server_requests,
    )
    .expect("translated resolved notification should succeed");
    assert_eq!(
        translated,
        Some(JSONRPCNotification {
            method: "serverRequest/resolved".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-1",
                "requestId": gateway_request_id,
            })),
        })
    );
    assert_eq!(resolved_server_requests.is_empty(), true);

    let duplicate = super::super::super::server_notification_to_jsonrpc(
        notification,
        &request_context,
        &observability,
        worker_id,
        "ws://worker-b.invalid",
        &mut resolved_server_requests,
    )
    .expect("duplicate resolved notification should succeed");
    assert_eq!(duplicate, None);
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "downstream_server_request_resolved",
                "serverRequest/resolved",
                1,
            ),
            ("duplicate_resolved_replay", "serverRequest/resolved", 1),
        ],
    );
}

#[test]
fn collect_server_request_cleanup_for_worker_tracks_thread_and_connection_scoped_routes() {
    let worker_id = Some(3);
    let other_worker_id = Some(9);
    let mut pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-thread-pending".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id,
                worker_websocket_url: test_worker_websocket_url(worker_id),
                downstream_request_id: RequestId::String("downstream-thread-pending".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            RequestId::String("gateway-connection-pending".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id,
                worker_websocket_url: test_worker_websocket_url(worker_id),
                downstream_request_id: RequestId::String(
                    "downstream-connection-pending".to_string(),
                ),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
        (
            RequestId::String("gateway-other-worker".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: other_worker_id,
                worker_websocket_url: test_worker_websocket_url(other_worker_id),
                downstream_request_id: RequestId::String("downstream-other-worker".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-other".to_string()),
            },
        ),
    ]);
    let mut resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id,
                request_id: RequestId::String("downstream-thread-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-thread-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id,
                request_id: RequestId::String("downstream-connection-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-connection-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: other_worker_id,
                request_id: RequestId::String("downstream-other-worker-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-other-worker-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(other_worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-other".to_string()),
            },
        ),
    ]);

    let cleanup = super::super::super::collect_server_request_cleanup_for_worker(
        &mut pending_server_requests,
        &mut resolved_server_requests,
        worker_id,
    );

    assert_eq!(cleanup.resolved_thread_scoped_requests, 2);
    assert_eq!(cleanup.stranded_connection_scoped_requests, 2);
    assert_eq!(
        cleanup.resolved_thread_scoped_request_ids,
        vec![
            RequestId::String("gateway-thread-pending".to_string()),
            RequestId::String("gateway-thread-resolved".to_string()),
        ]
    );
    assert_eq!(
        cleanup.resolved_thread_scoped_thread_ids,
        vec!["thread-owned".to_string(), "thread-owned".to_string()]
    );
    assert_eq!(
        cleanup.resolved_thread_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    assert_eq!(
        cleanup.stranded_connection_scoped_request_ids,
        vec![
            RequestId::String("gateway-connection-pending".to_string()),
            RequestId::String("gateway-connection-resolved".to_string()),
        ]
    );
    assert_eq!(
        cleanup.stranded_connection_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    assert_eq!(
        cleanup.resolved_notifications,
        vec![
            super::super::super::WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-owned".to_string(),
                    request_id: RequestId::String("gateway-thread-pending".to_string()),
                },
                method: "item/tool/requestUserInput".to_string(),
            },
            super::super::super::WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-owned".to_string(),
                    request_id: RequestId::String("gateway-thread-resolved".to_string()),
                },
                method: "item/tool/requestUserInput".to_string(),
            },
        ]
    );
    assert_eq!(cleanup.has_stranded_connection_scoped_requests(), true);
    assert_eq!(pending_server_requests.len(), 1);
    assert_eq!(
        pending_server_requests
            .contains_key(&RequestId::String("gateway-other-worker".to_string())),
        true
    );
    assert_eq!(resolved_server_requests.len(), 1);
    assert_eq!(
        resolved_server_requests.contains_key(&super::super::super::DownstreamServerRequestKey {
            worker_id: other_worker_id,
            request_id: RequestId::String("downstream-other-worker-resolved".to_string()),
        }),
        true
    );
}

#[test]
fn record_worker_server_request_cleanup_metrics_records_cleanup_counts() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let cleanup = super::super::super::WorkerServerRequestCleanup {
        resolved_thread_scoped_requests: 2,
        resolved_thread_scoped_methods: vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ],
        stranded_connection_scoped_requests: 1,
        stranded_connection_scoped_methods: vec!["account/chatgptAuthTokens/refresh".to_string()],
        ..Default::default()
    };

    super::super::super::record_worker_server_request_cleanup_metrics(&observability, &cleanup);

    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "worker_cleanup_resolved_thread_scoped",
                "item/tool/requestUserInput",
                2,
            ),
            (
                "worker_cleanup_stranded_connection_scoped",
                "account/chatgptAuthTokens/refresh",
                1,
            ),
        ],
    );
}

#[test]
fn record_client_server_request_cleanup_metrics_records_cleanup_counts() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);

    super::super::super::record_client_server_request_cleanup_metrics(
        &observability,
        &HashMap::from([
            (
                RequestId::String("gateway-thread-pending".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String(
                        "downstream-thread-pending".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-owned".to_string()),
                },
            ),
            (
                RequestId::String("gateway-connection-pending".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String(
                        "downstream-connection-pending".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: None,
                },
            ),
        ]),
        &HashMap::from([(
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(0),
                request_id: RequestId::String("downstream-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(Some(0)),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        )]),
    );

    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejected_connection_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_answered_but_unresolved",
                "item/tool/requestUserInput",
                1,
            ),
        ],
    );
}

#[tokio::test]
async fn reject_pending_server_requests_records_successful_cleanup_delivery() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut pending_server_requests = HashMap::from([(
        RequestId::String("gateway-delivered".to_string()),
        super::super::super::PendingServerRequestRoute {
            worker_id: Some(0),
            worker_websocket_url: test_worker_websocket_url(Some(0)),
            downstream_request_id: RequestId::String("downstream-delivered".to_string()),
            method: "item/tool/requestUserInput".to_string(),
            thread_id: Some("thread-owned".to_string()),
        },
    )]);

    super::super::super::reject_pending_server_requests(
        &router,
        &observability,
        &request_context,
        "client_disconnected",
        Some("test cleanup"),
        &mut pending_server_requests,
        &HashMap::new(),
    )
    .await
    .expect("active downstream request handle should accept cleanup rejection");

    assert!(pending_server_requests.is_empty());
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_delivered",
                "item/tool/requestUserInput",
                1,
            ),
        ],
    );

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reject_pending_server_requests_records_failed_cleanup_delivery() {
    let (client, request_handle) = start_test_request_handle().await;
    client.shutdown().await.expect("client should shut down");

    let (event_tx, event_rx) = mpsc::channel(1);
    let router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://worker-a.invalid".to_string(),
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
                            websocket_url: "ws://worker-b.invalid".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };

    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-failed".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(0),
                worker_websocket_url: test_worker_websocket_url(Some(0)),
                downstream_request_id: RequestId::String("downstream-failed".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            RequestId::String("gateway-skipped".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: test_worker_websocket_url(Some(1)),
                downstream_request_id: RequestId::String("downstream-skipped".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
    ]);

    let logs = capture_logs_async(async {
        let err = super::super::super::reject_pending_server_requests(
            &router,
            &observability,
            &request_context,
            "client_disconnected",
            Some("test cleanup"),
            &mut pending_server_requests,
            &HashMap::new(),
        )
        .await
        .expect_err("closed downstream request handle should fail cleanup");

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    })
    .await;

    assert!(pending_server_requests.is_empty());
    assert!(
            logs.contains("skipping pending server-request rejection because the downstream worker route is unavailable")
        );
    assert!(logs.contains("failed to reject pending downstream server request"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejected_connection_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_failed",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_skipped_unavailable_worker",
                "item/tool/requestUserInput",
                1,
            ),
        ],
        "item/tool/requestUserInput",
        2,
    );
}

#[test]
fn worker_cleanup_records_failed_synthesized_resolved_delivery() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_id = RequestId::String("gateway-srv-1".to_string());
    let err = io::Error::new(io::ErrorKind::TimedOut, "gateway websocket send timed out");

    let logs = capture_logs(|| {
        super::super::super::record_worker_cleanup_resolution_send_failure(
            &observability,
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            &super::super::super::WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-visible".to_string(),
                    request_id,
                },
                method: "item/tool/requestUserInput".to_string(),
            },
            &err,
        );
    });

    assert!(
        logs.contains("failed to deliver synthesized serverRequest/resolved during worker cleanup")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("gateway_request_id=String(\"gateway-srv-1\")"));
    assert!(logs.contains("method=\"item/tool/requestUserInput\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert_worker_cleanup_resolution_send_failure_metrics(
        &metrics,
        "worker_cleanup_resolution_send_failed",
        "item/tool/requestUserInput",
        "client_send_timed_out",
    );
}

#[test]
fn aggregated_page_bounds_returns_requested_window_when_offset_is_in_range() {
    assert_eq!(
        aggregated_page_bounds(5, 1, 2, "apps-offset:"),
        (1, 3, Some("apps-offset:3".to_string()))
    );
}

#[test]
fn aggregated_page_bounds_returns_empty_page_when_offset_is_past_end() {
    assert_eq!(
        aggregated_page_bounds(2, 5, 3, "apps-offset:"),
        (2, 2, None)
    );
}

#[test]
fn connection_errors_that_end_the_northbound_socket_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidData,
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::UnexpectedEof,
        std::io::ErrorKind::WriteZero,
        std::io::ErrorKind::NotConnected,
    ] {
        let err = std::io::Error::new(kind, "test");
        assert_eq!(
            super::super::super::should_reject_pending_server_requests_after_connection_error(&err),
            true
        );
    }
}

#[test]
fn non_terminal_connection_errors_do_not_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidInput,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::Other,
    ] {
        let err = std::io::Error::new(kind, "test");
        assert_eq!(
            super::super::super::should_reject_pending_server_requests_after_connection_error(&err),
            false
        );
    }
}

#[test]
fn classify_v2_connection_error_maps_timeout_and_disconnect_kinds() {
    assert_eq!(
        super::super::super::classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        )),
        "client_send_timed_out"
    );
    assert_eq!(
        super::super::super::classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "closed",
        )),
        "client_disconnected"
    );
    assert_eq!(
        super::super::super::classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid",
        )),
        "protocol_violation"
    );
    assert_eq!(
        super::super::super::classify_v2_connection_error(&std::io::Error::other("other")),
        "connection_error"
    );
}

#[test]
fn observe_v2_connection_records_client_send_timeout_outcome() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let context = GatewayRequestContext::default();
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();
    observability
        .v2_connection_health()
        .update_connection_pending_counts(
            connection_id,
            super::super::super::GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

    super::super::super::observe_v2_connection(
        &observability,
        connection_id,
        &context,
        super::super::super::classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "gateway websocket send timed out",
        )),
        None,
        super::super::super::GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 4,
            pending_client_request_worker_counts: Vec::new(),
            pending_client_request_method_counts: vec![
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 3,
                },
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 1,
                },
            ],
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: Vec::new(),
            server_request_backlog_method_counts: Vec::new(),
        },
        Duration::from_millis(9),
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.active_connection_count, 0);
    assert_eq!(
        health_snapshot.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(health_snapshot.last_connection_detail, None);
    assert_eq!(
        health_snapshot.last_connection_pending_client_request_count,
        4
    );
    assert_eq!(
        health_snapshot.last_connection_max_pending_client_request_count,
        6
    );
    assert_eq!(
        health_snapshot
            .last_connection_pending_client_request_started_at
            .is_some(),
        true
    );
    assert_eq!(
        health_snapshot.last_connection_pending_client_request_method_counts,
        vec![
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 3,
            },
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "thread/read".to_string(),
                pending_client_request_count: 1,
            },
        ]
    );
    assert_eq!(
        health_snapshot.last_connection_pending_server_request_count,
        2
    );
    assert_eq!(
        health_snapshot.last_connection_answered_but_unresolved_server_request_count,
        1
    );
    assert_eq!(
        health_snapshot.last_connection_server_request_backlog_count,
        3
    );
    assert_eq!(
        health_snapshot.last_connection_max_server_request_backlog_count,
        5
    );
    assert_eq!(health_snapshot.last_connection_completed_at.is_some(), true);
    assert_eq!(health_snapshot.last_connection_duration_ms, Some(9));

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_method_points = Vec::new();
    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 9.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            "gateway_v2_connection_pending_client_requests" => {
                saw_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 4.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending client request aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending client request type"),
                }
            }
            "gateway_v2_connection_max_pending_client_requests" => {
                saw_max_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 6.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max pending client request aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max pending client request type"),
                }
            }
            "gateway_v2_connection_pending_client_requests_by_method" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        pending_client_request_method_points.extend(histogram.data_points().map(
                            |point| {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                (attributes, point.count(), point.sum())
                            },
                        ));
                    }
                    _ => panic!(
                        "unexpected v2 connection pending client request by method aggregation"
                    ),
                },
                _ => panic!("unexpected v2 connection pending client request by method type"),
            },
            "gateway_v2_connection_pending_server_requests" => {
                saw_pending_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending type"),
                }
            }
            "gateway_v2_connection_answered_but_unresolved_server_requests" => {
                saw_answered_but_unresolved_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection answered-but-unresolved aggregation"),
                    },
                    _ => panic!("unexpected v2 connection answered-but-unresolved type"),
                }
            }
            "gateway_v2_connection_server_request_backlog" => {
                saw_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection server-request backlog aggregation"),
                    },
                    _ => panic!("unexpected v2 connection server-request backlog type"),
                }
            }
            "gateway_v2_connection_max_server_request_backlog" => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 5.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max server-request backlog aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max server-request backlog type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                3.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests);
    assert!(saw_answered_but_unresolved_server_requests);
    assert!(saw_server_request_backlog);
    assert!(saw_max_server_request_backlog);
}

#[test]
fn record_project_worker_route_selected_updates_v2_connection_health_snapshot() {
    let observability = GatewayObservability::default();

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        Some("acct-b"),
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.project_worker_route_selection_count, 1);
    assert_eq!(
        health_snapshot.project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 1,
            }
        ]
    );
    assert_eq!(
        health_snapshot.last_project_worker_route_selected_worker_id,
        Some(7)
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_account_id
            .as_deref(),
        Some("acct-b")
    );
    assert!(
        health_snapshot
            .last_project_worker_route_selected_at
            .is_some()
    );
}

#[test]
fn record_project_worker_route_selected_tracks_missing_account_id_in_v2_connection_health_snapshot()
{
    let observability = GatewayObservability::default();

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        None,
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.project_worker_route_selection_count, 1);
    assert_eq!(
        health_snapshot.project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 1,
            }
        ]
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_account_id
            .as_deref(),
        None
    );
    assert!(
        health_snapshot
            .last_project_worker_route_selected_at
            .is_some()
    );
}

#[test]
fn observe_v2_connection_emits_terminal_detail_in_connection_log() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();

    let logs = capture_logs(|| {
        super::super::super::observe_v2_connection(
            &observability,
            connection_id,
            &context,
            "protocol_violation",
            Some("unexpected gateway websocket server-request response"),
            super::super::super::GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
            Duration::from_millis(13),
        );
    });

    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("protocol_violation"));
    assert!(logs.contains("unexpected gateway websocket server-request response"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("13"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=2"));
}

pub(crate) fn assert_v2_server_request_rejection_and_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    rejection_method: &str,
    rejection_reason: &str,
    expected_lifecycle: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_server_request_rejection_count = false;
    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_rejections" => {
                saw_server_request_rejection_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("method".to_string(), rejection_method.to_string()),
                                    ("reason".to_string(), rejection_reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected server-request rejection count aggregation"),
                    },
                    _ => panic!("unexpected server-request rejection count type"),
                }
            }
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            _ => {}
        }
    }
    assert!(saw_server_request_rejection_count);
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
}

pub(crate) fn assert_command_exec_pending_client_limit_metrics(
    metrics: &codex_otel::MetricsClient,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    let mut saw_client_request_rejection_count = false;

    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            "gateway_v2_client_request_rejections" => {
                saw_client_request_rejection_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("method".to_string(), "command/exec".to_string()),
                                    ("reason".to_string(), "pending_limit".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected client-request rejection count aggregation"),
                    },
                    _ => panic!("unexpected client-request rejection count type"),
                }
            }
            _ => {}
        }
    }

    let expected = [
        ("initialize", "ok", 1),
        ("command/exec", "rate_limited", 1),
        ("command/exec", "ok", 1),
    ];
    for (method, outcome, count) in expected {
        assert_eq!(
            observed_request_counts.get(&(method.to_string(), outcome.to_string())),
            Some(&count),
            "missing v2 request count metric for method={method} outcome={outcome}"
        );
        assert_eq!(
            observed_request_durations.get(&(method.to_string(), outcome.to_string())),
            Some(&count),
            "missing v2 request duration metric for method={method} outcome={outcome}"
        );
    }
    assert!(saw_client_request_rejection_count);
}

pub(crate) fn in_memory_metrics() -> codex_otel::MetricsClient {
    codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics")
}

pub(crate) fn assert_v2_request_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_counts = BTreeMap::new();
    let mut observed_durations = BTreeMap::new();
    let mut saw_request_count = false;
    let mut saw_request_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_request_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            for point in sum.data_points() {
                                let attributes = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                let (method, outcome) = request_metric_point_tags(attributes);
                                *observed_counts.entry((method, outcome)).or_insert(0) +=
                                    point.value();
                            }
                        }
                        _ => panic!("unexpected v2 request count aggregation"),
                    },
                    _ => panic!("unexpected v2 request count type"),
                }
            }
            "gateway_v2_request_duration" => {
                saw_request_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            for point in histogram.data_points() {
                                let attributes = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                let (method, outcome) = request_metric_point_tags(attributes);
                                *observed_durations.entry((method, outcome)).or_insert(0) +=
                                    point.count();
                            }
                        }
                        _ => panic!("unexpected v2 request duration aggregation"),
                    },
                    _ => panic!("unexpected v2 request duration type"),
                }
            }
            _ => {}
        }
    }
    assert!(saw_request_count);
    assert!(saw_request_duration);
    assert_eq!(
        observed_counts.values().copied().sum::<u64>(),
        expected.iter().map(|(_, _, count)| *count).sum::<u64>()
    );
    assert_eq!(
        observed_durations.values().copied().sum::<u64>(),
        expected.iter().map(|(_, _, count)| *count).sum::<u64>()
    );
    for (method, outcome, count) in expected {
        assert_eq!(
            observed_counts.get(&(method.to_string(), outcome.to_string())),
            Some(count),
            "missing v2 request count metric for method={method} outcome={outcome}"
        );
        assert_eq!(
            observed_durations.get(&(method.to_string(), outcome.to_string())),
            Some(count),
            "missing v2 request duration metric for method={method} outcome={outcome}"
        );
    }
}

fn request_metric_point_tags(attributes: BTreeMap<String, String>) -> (String, String) {
    let method = attributes
        .get("method")
        .expect("request metric should have method")
        .clone();
    let outcome = attributes
        .get("outcome")
        .expect("request metric should have outcome")
        .clone();
    (method, outcome)
}

pub(crate) fn assert_repeated_initialize_metrics(metrics: &codex_otel::MetricsClient) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    let mut saw_protocol_violation_count = false;

    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            "gateway_v2_protocol_violations" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("phase".to_string(), "post_initialize".to_string()),
                                ("reason".to_string(), "repeated_initialize".to_string()),
                            ])
                        );
                        saw_protocol_violation_count = true;
                    }
                    _ => panic!("unexpected protocol violation count aggregation"),
                },
                _ => panic!("unexpected protocol violation count type"),
            },
            _ => {}
        }
    }

    let expected_request_metrics = BTreeMap::from([
        (("initialize".to_string(), "ok".to_string()), 1),
        (("initialize".to_string(), "invalid_request".to_string()), 1),
    ]);
    assert_eq!(observed_request_counts, expected_request_metrics);
    assert_eq!(observed_request_durations, expected_request_metrics);
    assert!(saw_protocol_violation_count);
}

#[test]
fn observe_v2_request_records_rate_limited_metrics_and_audit_log() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let context = GatewayRequestContext {
        tenant_id: "tenant-quota".to_string(),
        project_id: Some("project-quota".to_string()),
    };

    let logs = capture_logs(|| {
        super::super::super::observe_v2_request(
            &observability,
            &context,
            "turn/start",
            "rate_limited",
            Duration::from_millis(7),
        );
    });

    assert!(logs.contains("codex_gateway.audit"));
    assert!(logs.contains("gateway v2 request completed"));
    assert!(logs.contains("method=\"turn/start\""));
    assert!(logs.contains("outcome=\"rate_limited\""));
    assert!(logs.contains("tenant_id=\"tenant-quota\""));
    assert!(logs.contains("project_id=\"project-quota\""));
    assert_v2_request_metrics(&metrics, &[("turn/start", "rate_limited", 1)]);
}

pub(crate) fn assert_v2_fail_closed_request_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_fail_closed_request_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_fail_closed_requests" {
            saw_fail_closed_request_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected fail-closed request count aggregation"),
                },
                _ => panic!("unexpected fail-closed request count type"),
            }
        }
    }
    assert!(saw_fail_closed_request_count);
}

pub(crate) fn assert_v2_degraded_thread_discovery_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_degraded_thread_discovery_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_degraded_thread_discovery" {
            saw_degraded_thread_discovery_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected degraded thread discovery count aggregation"),
                },
                _ => panic!("unexpected degraded thread discovery count type"),
            }
        }
    }
    assert!(saw_degraded_thread_discovery_count);
}

pub(crate) fn assert_v2_thread_list_deduplication_metric(
    metrics: &codex_otel::MetricsClient,
    selected_worker_id: Option<usize>,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let selected_worker_id =
        selected_worker_id.map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
    let mut saw_thread_list_deduplication_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_thread_list_deduplications" {
            saw_thread_list_deduplication_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([(
                                "selected_worker_id".to_string(),
                                selected_worker_id.clone(),
                            )])
                        );
                    }
                    _ => panic!("unexpected thread-list deduplication count aggregation"),
                },
                _ => panic!("unexpected thread-list deduplication count type"),
            }
        }
    }
    assert!(saw_thread_list_deduplication_count);
}

pub(crate) fn assert_v2_thread_route_recovery_metric(
    metrics: &codex_otel::MetricsClient,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_thread_route_recovery_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_thread_route_recoveries" {
            saw_thread_route_recovery_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([("outcome".to_string(), outcome.to_string())])
                        );
                    }
                    _ => panic!("unexpected thread route recovery count aggregation"),
                },
                _ => panic!("unexpected thread route recovery count type"),
            }
        }
    }
    assert!(saw_thread_route_recovery_count);
}

pub(crate) fn assert_no_v2_metric(metrics: &codex_otel::MetricsClient, metric_name: &str) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    for metric in metrics {
        assert_ne!(metric.name(), metric_name);
    }
}

pub(crate) fn assert_v2_upstream_request_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_upstream_request_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_upstream_request_failures" {
            saw_upstream_request_failure_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected upstream request failure count aggregation"),
                },
                _ => panic!("unexpected upstream request failure count type"),
            }
        }
    }
    assert!(saw_upstream_request_failure_count);
}

pub(crate) fn assert_v2_suppressed_notification_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reason: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_suppressed_notification_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_suppressed_notifications" {
            saw_suppressed_notification_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                ("reason".to_string(), reason.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected suppressed notification count aggregation"),
                },
                _ => panic!("unexpected suppressed notification count type"),
            }
        }
    }
    assert!(saw_suppressed_notification_count);
}

pub(crate) fn assert_no_v2_suppressed_notification_metric(metrics: &codex_otel::MetricsClient) {
    assert_no_v2_metric(metrics, "gateway_v2_suppressed_notifications");
}

pub(crate) fn assert_v2_forwarded_notification_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    expected_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_forwarded_notification_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_forwarded_notifications" {
            saw_forwarded_notification_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), expected_count);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([("method".to_string(), method.to_string())])
                        );
                    }
                    _ => panic!("unexpected forwarded notification count aggregation"),
                },
                _ => panic!("unexpected forwarded notification count type"),
            }
        }
    }
    assert!(saw_forwarded_notification_count);
}

pub(crate) fn assert_no_v2_forwarded_notification_metric(metrics: &codex_otel::MetricsClient) {
    assert_no_v2_metric(metrics, "gateway_v2_forwarded_notifications");
}

pub(crate) fn assert_v2_notification_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_notification_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_notification_send_failures" {
            saw_notification_send_failure_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            }
        }
    }
    assert!(saw_notification_send_failure_count);
}

pub(crate) fn assert_v2_client_response_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_response_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_client_response_send_failures" {
            saw_response_send_failure_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), method.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected client response send failure count aggregation"),
                },
                _ => panic!("unexpected client response send failure count type"),
            }
        }
    }
    assert!(saw_response_send_failure_count);
}

pub(crate) fn assert_v2_close_frame_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    code: u16,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_close_frame_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_close_frame_send_failures" {
            saw_close_frame_send_failure_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("code".to_string(), code.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected close frame send failure count aggregation"),
                },
                _ => panic!("unexpected close frame send failure count type"),
            }
        }
    }
    assert!(saw_close_frame_send_failure_count);
}

pub(crate) fn assert_worker_cleanup_resolution_send_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    lifecycle_event: &str,
    lifecycle_method: &str,
    notification_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_lifecycle_count = false;
    let mut saw_notification_send_failure_count = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("lifecycle point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("event".to_string(), lifecycle_event.to_string()),
                                ("method".to_string(), lifecycle_method.to_string()),
                            ])
                        );
                        saw_lifecycle_count = true;
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_notification_send_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum
                            .data_points()
                            .next()
                            .expect("notification failure point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("method".to_string(), "serverRequest/resolved".to_string()),
                                ("outcome".to_string(), notification_outcome.to_string()),
                            ])
                        );
                        saw_notification_send_failure_count = true;
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            },
            _ => {}
        }
    }

    assert!(saw_lifecycle_count);
    assert!(saw_notification_send_failure_count);
}

pub(crate) fn assert_v2_server_request_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_points = expected
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_points.sort();

    let mut actual_points = Vec::new();
    for metric in metrics {
        if metric.name() == "gateway_v2_server_request_lifecycle_events" {
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            }
        }
    }
    actual_points.sort();
    assert_eq!(actual_points, expected_points);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_delivery_failures: &[(&str, u64)],
) {
    assert_v2_server_request_answer_account_exhaustion_metrics(
        metrics,
        expected_lifecycle,
        expected_delivery_failures,
        &[],
    );
}

pub(crate) fn assert_v2_server_request_answer_account_exhaustion_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_delivery_failures: &[(&str, u64)],
    expected_account_events: &[(usize, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();
    let mut expected_delivery_failure_points = expected_delivery_failures
        .iter()
        .map(|(response_kind, value)| {
            (
                BTreeMap::from([("response_kind".to_string(), (*response_kind).to_string())]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_delivery_failure_points.sort();
    let mut expected_account_points = expected_account_events
        .iter()
        .map(|(worker_id, event, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("worker_id".to_string(), worker_id.to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_account_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    let mut actual_delivery_failure_points = Vec::new();
    let mut actual_account_points = Vec::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_server_request_answer_delivery_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_delivery_failure_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => {
                        panic!("unexpected server-request answer delivery failure aggregation")
                    }
                },
                _ => panic!("unexpected server-request answer delivery failure count type"),
            },
            "gateway_v2_account_capacity_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_account_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected account capacity event aggregation"),
                },
                _ => panic!("unexpected account capacity event type"),
            },
            _ => {}
        }
    }
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
    actual_delivery_failure_points.sort();
    assert_eq!(
        actual_delivery_failure_points,
        expected_delivery_failure_points
    );
    actual_account_points.sort();
    assert_eq!(actual_account_points, expected_account_points);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_method: &str,
    expected_delivery_failure_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    let mut saw_delivery_failure_count = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_server_request_rejection_delivery_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), expected_delivery_failure_count);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([("method".to_string(), expected_method.to_string(),)])
                        );
                        saw_delivery_failure_count = true;
                    }
                    _ => panic!("unexpected server-request rejection delivery failure aggregation"),
                },
                _ => panic!("unexpected server-request rejection delivery failure count type"),
            },
            _ => {}
        }
    }
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
    assert!(saw_delivery_failure_count);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_connection_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_event: &str,
    expected_method: &str,
    expected_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_lifecycle_event = false;
    let mut saw_connection_count = false;
    let mut saw_connection_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if attributes
                                == BTreeMap::from([
                                    ("event".to_string(), expected_event.to_string()),
                                    ("method".to_string(), expected_method.to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 1);
                                saw_lifecycle_event = true;
                            }
                        }
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_connections" => {
                saw_connection_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    expected_outcome.to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_connection_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("duration point");
                            assert_eq!(point.count(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    expected_outcome.to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_lifecycle_event);
    assert!(saw_connection_count);
    assert!(saw_connection_duration);
}

pub(crate) fn assert_v2_protocol_violation_and_connection_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut saw_connection_count = false;
    let mut saw_connection_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
            "gateway_v2_connections" => {
                saw_connection_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([("outcome".to_string(), outcome.to_string(),)])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_connection_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("duration point");
                            assert_eq!(point.count(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([("outcome".to_string(), outcome.to_string(),)])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_protocol_violation_count);
    assert!(saw_connection_count);
    assert!(saw_connection_duration);
}

pub(crate) fn assert_v2_protocol_violation_and_request_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    request_method: &str,
    request_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            _ => {}
        }
    }

    assert!(saw_protocol_violation_count);
    let expected_request = (request_method.to_string(), request_outcome.to_string());
    assert_eq!(observed_request_counts.get(&expected_request), Some(&1));
    assert_eq!(observed_request_durations.get(&expected_request), Some(&1));
}

pub(crate) fn assert_v2_protocol_violation_connection_and_request_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    connection_outcome: &str,
    request_method: &str,
    request_outcome: &str,
    expected_request_metric_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut observed_connection_counts = BTreeMap::new();
    let mut observed_connection_durations = BTreeMap::new();
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
            "gateway_v2_connections" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let outcome = attributes
                                .get("outcome")
                                .expect("connection metric should have outcome")
                                .clone();
                            *observed_connection_counts.entry(outcome).or_insert(0) +=
                                point.value();
                        }
                    }
                    _ => panic!("unexpected v2 connection count aggregation"),
                },
                _ => panic!("unexpected v2 connection count type"),
            },
            "gateway_v2_connection_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let outcome = attributes
                                .get("outcome")
                                .expect("connection metric should have outcome")
                                .clone();
                            *observed_connection_durations.entry(outcome).or_insert(0) +=
                                point.count();
                        }
                    }
                    _ => panic!("unexpected v2 connection duration aggregation"),
                },
                _ => panic!("unexpected v2 connection duration type"),
            },
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            _ => {}
        }
    }

    assert!(saw_protocol_violation_count);
    assert_eq!(
        observed_connection_counts.get(connection_outcome),
        Some(&1),
        "missing v2 connection count metric for outcome={connection_outcome}"
    );
    assert_eq!(
        observed_connection_durations.get(connection_outcome),
        Some(&1),
        "missing v2 connection duration metric for outcome={connection_outcome}"
    );
    let expected_request = (request_method.to_string(), request_outcome.to_string());
    assert_eq!(
        observed_request_counts.get(&expected_request),
        Some(&expected_request_metric_count),
        "missing v2 request count metric for method={request_method} outcome={request_outcome}"
    );
    assert_eq!(
        observed_request_durations.get(&expected_request),
        Some(&expected_request_metric_count),
        "missing v2 request duration metric for method={request_method} outcome={request_outcome}"
    );
}

pub(crate) fn assert_v2_downstream_backpressure_metric(
    metrics: &codex_otel::MetricsClient,
    worker_id: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_backpressure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_downstream_backpressure_events" {
            saw_backpressure_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([("worker_id".to_string(), worker_id.to_string()),])
                        );
                    }
                    _ => panic!("unexpected downstream backpressure count aggregation"),
                },
                _ => panic!("unexpected downstream backpressure count type"),
            }
        }
    }
    assert!(saw_backpressure_count);
}

pub(crate) fn assert_v2_client_send_timeout_and_server_request_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_client_send_timeout_count = false;
    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_client_send_timeouts" => {
                saw_client_send_timeout_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                        }
                        _ => panic!("unexpected client send timeout count aggregation"),
                    },
                    _ => panic!("unexpected client send timeout count type"),
                }
            }
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            _ => {}
        }
    }

    assert!(saw_client_send_timeout_count);
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
}

pub(crate) fn assert_v2_worker_reconnect_metric(
    metrics: &codex_otel::MetricsClient,
    worker_id: usize,
    outcome: &str,
) {
    assert_v2_worker_reconnect_metrics(metrics, &[(worker_id, outcome)]);
}

pub(crate) fn assert_v2_account_capacity_event_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(usize, &str)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_attributes = expected
        .iter()
        .map(|(worker_id, event)| {
            (
                BTreeMap::from([
                    ("worker_id".to_string(), worker_id.to_string()),
                    ("event".to_string(), (*event).to_string()),
                ]),
                false,
            )
        })
        .collect::<Vec<_>>();
    for metric in metrics {
        if metric.name() == "gateway_v2_account_capacity_events" {
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if let Some((_, seen)) = expected_attributes
                                .iter_mut()
                                .find(|(expected, _)| *expected == attributes)
                            {
                                *seen = true;
                                assert_eq!(point.value(), 1);
                            }
                        }
                    }
                    _ => panic!("unexpected account capacity event aggregation"),
                },
                _ => panic!("unexpected account capacity event type"),
            }
        }
    }

    let missing = expected_attributes
        .into_iter()
        .filter_map(|(attributes, seen)| (!seen).then_some(attributes))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing gateway_v2_account_capacity_events metric points: {missing:?}"
    );
}

pub(crate) fn assert_v2_account_capacity_event_metric_count(
    metrics: &codex_otel::MetricsClient,
    worker_id: usize,
    event: &str,
    expected_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    for metric in metrics {
        if metric.name() != "gateway_v2_account_capacity_events" {
            continue;
        }
        match metric.data() {
            AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                for point in sum.data_points() {
                    let attributes: BTreeMap<String, String> = point
                        .attributes()
                        .map(|attribute| {
                            (
                                attribute.key.as_str().to_string(),
                                attribute.value.as_str().to_string(),
                            )
                        })
                        .collect();
                    if attributes
                        == BTreeMap::from([
                            ("worker_id".to_string(), worker_id.to_string()),
                            ("event".to_string(), event.to_string()),
                        ])
                    {
                        assert_eq!(point.value(), expected_count);
                        return;
                    }
                }
            }
            AggregatedMetrics::U64(_) => {
                panic!("unexpected account capacity event aggregation")
            }
            _ => panic!("unexpected account capacity event type"),
        }
    }

    panic!(
        "missing gateway_v2_account_capacity_events metric point for worker {worker_id} and event {event}"
    );
}

pub(crate) fn assert_v2_account_capacity_event_health(
    observability: &GatewayObservability,
    worker_id: usize,
    event: &str,
    count: usize,
    tenant_id: &str,
    project_id: Option<&str>,
    reason: &str,
) {
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [(event.to_string(), count)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(worker_id, [(event.to_string(), count)].into())]
    );
    assert_eq!(health.last_account_capacity_event.as_deref(), Some(event));
    assert_eq!(
        health.last_account_capacity_event_worker_id,
        Some(worker_id)
    );
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some(tenant_id)
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        project_id
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(reason)
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
}

pub(crate) fn assert_v2_worker_reconnect_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(usize, &str)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_attributes = BTreeMap::new();
    for (worker_id, outcome) in expected {
        let (count, _) = expected_attributes
            .entry(BTreeMap::from([
                ("worker_id".to_string(), worker_id.to_string()),
                ("outcome".to_string(), (*outcome).to_string()),
            ]))
            .or_insert((0_u64, false));
        *count += 1;
    }
    for metric in metrics {
        if metric.name() == "gateway_v2_worker_reconnects" {
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if let Some((expected_count, seen)) =
                                expected_attributes.get_mut(&attributes)
                            {
                                *seen = true;
                                assert_eq!(point.value(), *expected_count);
                            }
                        }
                    }
                    _ => panic!("unexpected worker reconnect count aggregation"),
                },
                _ => panic!("unexpected worker reconnect count type"),
            }
        }
    }
    let missing = expected_attributes
        .into_iter()
        .filter_map(|(attributes, (_, seen))| (!seen).then_some(attributes))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing gateway_v2_worker_reconnects metric points: {missing:?}"
    );
}

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
                    assert_eq!(
                        error.error.code,
                        super::super::super::RATE_LIMITED_ERROR_CODE
                    );
                    assert_eq!(
                        error.error.message,
                        super::super::super::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
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
    assert_eq!(error.error.code, super::super::super::INVALID_PARAMS_CODE);
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
    assert_eq!(
        error.error.code,
        super::super::super::RATE_LIMITED_ERROR_CODE
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
    assert_eq!(
        error.error.code,
        super::super::super::RATE_LIMITED_ERROR_CODE
    );
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
    assert_eq!(
        path_error.error.code,
        super::super::super::INVALID_PARAMS_CODE
    );
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
    assert_eq!(error.error.code, super::super::super::INVALID_PARAMS_CODE);
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

#[tokio::test]
async fn websocket_upgrade_emits_v2_request_metrics() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: Some(0),
            turn_start_quota_per_minute: None,
        }),
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

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 2);
                            let mut saw_initialize = false;
                            let mut saw_model_list = false;
                            for point in sum.data_points() {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "initialize".to_string()),
                                        ("outcome".to_string(), "ok".to_string()),
                                    ])
                                {
                                    saw_initialize = true;
                                }
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "model/list".to_string()),
                                        ("outcome".to_string(), "rate_limited".to_string()),
                                    ])
                                {
                                    saw_model_list = true;
                                }
                            }
                            assert_eq!(saw_initialize, true);
                            assert_eq!(saw_model_list, true);
                        }
                        _ => panic!("unexpected v2 count aggregation"),
                    },
                    _ => panic!("unexpected v2 count type"),
                }
            }
            "gateway_v2_request_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 2);
                            let mut saw_initialize = false;
                            let mut saw_model_list = false;
                            for point in histogram.data_points() {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "initialize".to_string()),
                                        ("outcome".to_string(), "ok".to_string()),
                                    ])
                                {
                                    saw_initialize = true;
                                }
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "model/list".to_string()),
                                        ("outcome".to_string(), "rate_limited".to_string()),
                                    ])
                                {
                                    saw_model_list = true;
                                }
                            }
                            assert_eq!(saw_initialize, true);
                            assert_eq!(saw_model_list, true);
                        }
                        _ => panic!("unexpected v2 duration aggregation"),
                    },
                    _ => panic!("unexpected v2 duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_emits_v2_connection_metrics_for_initialize_timeout() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: start_mock_remote_server_for_initialize().await,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            test_initialize_response().await,
        ))),
        timeouts: GatewayV2Timeouts {
            initialize: Duration::from_millis(50),
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let (_websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_request_count = false;
    let mut saw_request_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_request_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("request count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                request_metric_point_tags(attributes),
                                ("initialize".to_string(), "timed_out".to_string())
                            );
                        }
                        _ => panic!("unexpected v2 request count aggregation"),
                    },
                    _ => panic!("unexpected v2 request count type"),
                }
            }
            "gateway_v2_request_duration" => {
                saw_request_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram
                                .data_points()
                                .next()
                                .expect("request duration point");
                            assert_eq!(point.count(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                request_metric_point_tags(attributes),
                                ("initialize".to_string(), "timed_out".to_string())
                            );
                        }
                        _ => panic!("unexpected v2 request duration aggregation"),
                    },
                    _ => panic!("unexpected v2 request duration type"),
                }
            }
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 1);
                            let point = sum.data_points().next().expect("count point");
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "initialize_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 1);
                            let point = histogram.data_points().next().expect("histogram point");
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "initialize_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);
    assert_eq!(saw_request_count, true);
    assert_eq!(saw_request_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_emits_v2_connection_metrics_for_downstream_disconnect() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_that_disconnects_after_initialize().await;
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
    let _ = wait_for_close_frame(&mut websocket).await;

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 1);
                            let point = sum.data_points().next().expect("count point");
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 1);
                            let point = histogram.data_points().next().expect("histogram point");
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_with_reason_when_downstream_disconnects() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_that_disconnects_after_initialize().await;
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

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::ERROR
    );
    assert_eq!(
        close_frame
            .reason
            .starts_with("downstream app-server disconnected:"),
        true
    );

    server_task.abort();
    let _ = server_task.await;
}
