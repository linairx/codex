use super::*;
use crate::northbound::v2::INVALID_REQUEST_CODE;
use pretty_assertions::assert_eq;

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
        assert_eq!(error.error.code, INVALID_REQUEST_CODE);
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
    assert_eq!(error.error.code, INVALID_REQUEST_CODE);
    assert_eq!(error.error.message, "connection is already initialized");
    assert_repeated_initialize_metrics(&metrics);

    server_task.abort();
    let _ = server_task.await;
}
