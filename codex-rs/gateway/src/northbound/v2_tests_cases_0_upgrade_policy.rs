use super::*;
use pretty_assertions::assert_eq;

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
