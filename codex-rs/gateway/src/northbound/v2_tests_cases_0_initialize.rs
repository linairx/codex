use super::*;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::INVALID_REQUEST_CODE;
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
        assert_eq!(error.error.code, INVALID_REQUEST_CODE);
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
    assert_eq!(error.error.code, INVALID_REQUEST_CODE);
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
        assert_eq!(error.error.code, INVALID_REQUEST_CODE);
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
        assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
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
