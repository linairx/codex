use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_runtime_healthz_reports_exec_server_execution_mode() {
    let exec_server_url = start_mock_exec_server().await;
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            exec_server_url: Some(exec_server_url),
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
    assert_eq!(health.status, GatewayHealthStatus::Ok);
    assert_eq!(health.runtime_mode, "embedded");
    assert_eq!(health.execution_mode, GatewayExecutionMode::ExecServer);
    assert_eq!(health.v2_transport.initialize_timeout_seconds, 30);
    assert_eq!(health.v2_transport.client_send_timeout_seconds, 10);
    assert_eq!(health.v2_transport.reconnect_retry_backoff_seconds, 1);
    assert_eq!(health.v2_transport.max_pending_server_requests, 64);
    assert_eq!(health.v2_transport.max_pending_client_requests, 64);
    assert_eq!(health.v2_connections.active_connection_count, 0);
    assert_eq!(health.v2_connections.peak_active_connection_count, 0);
    assert_eq!(health.v2_connections.total_connection_count, 0);
    assert_eq!(health.v2_connections.last_connection_started_at, None);
    assert_eq!(health.v2_connections.last_connection_completed_at, None);
    assert_eq!(health.v2_connections.last_connection_duration_ms, None);
    assert_eq!(health.v2_connections.max_connection_duration_ms, None);
    assert_eq!(health.v2_connections.last_connection_outcome, None);
    assert_eq!(health.v2_connections.connection_outcome_counts, vec![]);
    assert_eq!(health.v2_connections.last_connection_detail, None);
    assert_eq!(
        health
            .v2_connections
            .last_connection_pending_server_request_count,
        0
    );
    assert_eq!(
        health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        0
    );
    assert_eq!(health.remote_workers, None);

    server.shutdown().await.expect("shutdown");
}
