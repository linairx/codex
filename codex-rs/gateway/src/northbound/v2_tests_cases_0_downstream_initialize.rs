use super::*;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use pretty_assertions::assert_eq;

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
    assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
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
        assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
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
    assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
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
    assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
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
