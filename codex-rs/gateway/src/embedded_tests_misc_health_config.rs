use super::super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn healthz_reports_configured_v2_transport_settings() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            v2_initialize_timeout: Duration::from_secs(7),
            v2_client_send_timeout: Duration::from_secs(3),
            v2_reconnect_retry_backoff: Duration::from_secs(5),
            v2_max_pending_server_requests: 11,
            v2_max_pending_client_requests: 13,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.v2_transport.initialize_timeout_seconds, 7);
    assert_eq!(health.v2_transport.client_send_timeout_seconds, 3);
    assert_eq!(health.v2_transport.reconnect_retry_backoff_seconds, 5);
    assert_eq!(health.v2_transport.max_pending_server_requests, 11);
    assert_eq!(health.v2_transport.max_pending_client_requests, 13);
    assert_eq!(health.v2_connections.active_connection_count, 0);
    assert_eq!(health.v2_connections.peak_active_connection_count, 0);
    assert_eq!(health.v2_connections.total_connection_count, 0);
    assert_eq!(health.v2_connections.last_connection_started_at, None);
    assert_eq!(health.v2_connections.last_connection_completed_at, None);
    assert_eq!(health.v2_connections.last_connection_duration_ms, None);
    assert_eq!(health.v2_connections.last_connection_outcome, None);
    assert_eq!(health.v2_connections.last_connection_detail, None);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_requires_websocket_configuration() {
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let result = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: None,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await;

    assert_eq!(result.is_err(), true);
    assert_eq!(
        result.err().expect("expected missing remote config").kind(),
        std::io::ErrorKind::InvalidInput
    );
}
