use super::super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn healthz_reports_v2_connection_activity_and_last_outcome() {
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
    let connection = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("client should connect");

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.v2_connections.active_connection_count, 1);
    assert_eq!(health.v2_connections.peak_active_connection_count, 1);
    assert_eq!(health.v2_connections.total_connection_count, 1);
    assert_eq!(
        health.v2_connections.last_connection_started_at.is_some(),
        true
    );
    assert_eq!(health.v2_connections.last_connection_completed_at, None);
    assert_eq!(health.v2_connections.last_connection_duration_ms, None);
    assert_eq!(health.v2_connections.max_connection_duration_ms, None);
    assert_eq!(health.v2_connections.last_connection_outcome, None);
    assert_eq!(health.v2_connections.connection_outcome_counts, vec![]);

    assert_remote_client_shutdown(connection.shutdown().await);

    let settled_health = timeout(Duration::from_secs(15), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.last_connection_outcome == Some("client_closed".to_string()) {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("connection health should settle");
    assert_eq!(settled_health.v2_connections.active_connection_count, 0);
    assert_eq!(
        settled_health.v2_connections.peak_active_connection_count,
        1
    );
    assert_eq!(settled_health.v2_connections.total_connection_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_closed".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_duration_ms
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.max_connection_duration_ms,
        settled_health.v2_connections.last_connection_duration_ms
    );
    assert_eq!(
        settled_health.v2_connections.connection_outcome_counts,
        vec![GatewayV2ConnectionOutcomeCounts {
            outcome: "client_closed".to_string(),
            count: 1,
        }]
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn healthz_reports_v2_connection_peak_and_total_across_multiple_sessions() {
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
    let connection_a = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test-a".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("first client should connect");

    let first_started_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 1
                && health.v2_connections.total_connection_count == 1
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("first connection health should settle");
    assert_eq!(
        first_started_health.v2_connections.active_connection_count,
        1
    );
    assert_eq!(
        first_started_health
            .v2_connections
            .peak_active_connection_count,
        1
    );
    assert_eq!(
        first_started_health.v2_connections.total_connection_count,
        1
    );
    let first_started_at = first_started_health
        .v2_connections
        .last_connection_started_at
        .expect("first connection start time");

    let connection_b = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test-b".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("second client should connect");

    let second_started_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 2
                && health.v2_connections.peak_active_connection_count == 2
                && health.v2_connections.total_connection_count == 2
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("second connection health should settle");
    let second_started_at = second_started_health
        .v2_connections
        .last_connection_started_at
        .expect("second connection start time");
    assert_eq!(second_started_at >= first_started_at, true);

    assert_remote_client_shutdown(connection_a.shutdown().await);

    let after_first_close_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 1
                && health.v2_connections.peak_active_connection_count == 2
                && health.v2_connections.total_connection_count == 2
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("post-close health should settle");
    assert_eq!(
        after_first_close_health
            .v2_connections
            .last_connection_outcome,
        Some("client_closed".to_string())
    );

    let connection_c = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test-c".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("third client should connect");

    let third_started_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 2
                && health.v2_connections.peak_active_connection_count == 2
                && health.v2_connections.total_connection_count == 3
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("third connection health should settle");
    let third_started_at = third_started_health
        .v2_connections
        .last_connection_started_at
        .expect("third connection start time");
    assert_eq!(third_started_at >= second_started_at, true);

    assert_remote_client_shutdown(connection_b.shutdown().await);
    assert_remote_client_shutdown(connection_c.shutdown().await);

    let settled_health = timeout(Duration::from_secs(15), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health.v2_connections.peak_active_connection_count == 2
                && health.v2_connections.total_connection_count == 3
                && health.v2_connections.last_connection_outcome.is_some()
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("final connection health should settle");
    assert_eq!(settled_health.v2_connections.active_connection_count, 0);
    assert_eq!(
        settled_health.v2_connections.peak_active_connection_count,
        2
    );
    assert_eq!(settled_health.v2_connections.total_connection_count, 3);
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_closed".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_duration_ms
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn healthz_reports_initialize_invalid_request_outcome() {
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
    let (mut websocket, response) = connect_async(format!("ws://{}/", server.local_addr()))
        .await
        .expect("websocket should connect");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);

    write_websocket_message(
        &mut websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize error response");
    };
    assert_eq!(error.id, RequestId::Integer(1));
    assert_eq!(error.error.message.contains("clientInfo"), true);

    let settled_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health.v2_connections.last_connection_outcome.is_some()
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("connection health should settle");
    assert_eq!(settled_health.runtime_mode, "embedded");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::Embedded
    );
    assert_eq!(settled_health.v2_connections.active_connection_count, 0);
    assert_eq!(
        settled_health.v2_connections.peak_active_connection_count,
        1
    );
    assert_eq!(settled_health.v2_connections.total_connection_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("initialize_invalid_request".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("clientInfo")),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_duration_ms
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}
