use super::*;
use crate::northbound::v2::INITIALIZE_TIMEOUT_CLOSE_REASON;
use crate::northbound::v2::INVALID_REQUEST_CODE;
use crate::northbound::v2::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION;
use crate::northbound::v2::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION;
use crate::northbound::v2_wire::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON;
use crate::northbound::v2_wire::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON;
use pretty_assertions::assert_eq;

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
            max_pending_server_requests: MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
            max_pending_client_requests: MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
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
        assert_eq!(close_frame.reason, INITIALIZE_TIMEOUT_CLOSE_REASON);
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
                .starts_with(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
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
        logs.contains(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON),
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
                .starts_with(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
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
        logs.contains(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON),
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
                .starts_with(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
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
        logs.contains(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON),
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
                .starts_with(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
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
        logs.contains(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON),
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
                    code: INVALID_REQUEST_CODE,
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
                    code: INVALID_REQUEST_CODE,
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
