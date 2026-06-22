use super::*;
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

#[tokio::test]
async fn remote_single_worker_healthz_reports_downstream_connect_error_outcome() {
    let worker = start_mock_remote_server_that_closes_before_initialize_response().await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker,
                    auth_token: None,
                    account_id: None,
                }],
            }),
            v2_initialize_timeout: Duration::from_millis(200),
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
            params: Some(serde_json::json!({
                "clientInfo": {
                    "name": "codex-gateway-test",
                    "version": "0.0.0-test",
                },
                "capabilities": null,
            })),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected downstream connect error response");
    };
    assert_eq!(error.id, RequestId::Integer(1));
    assert_eq!(
        error
            .error
            .message
            .contains("gateway failed to connect downstream app-server"),
        true
    );

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
    assert_eq!(settled_health.runtime_mode, "remote");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
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
        Some("downstream_connect_error".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_detail
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_healthz_reports_v2_connection_activity_and_last_outcome() {
    let worker =
        start_mock_remote_server_for_idle_v2_sessions(Some("secret-token".to_string())).await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker,
                    auth_token: Some("secret-token".to_string()),
                    account_id: None,
                }],
            }),
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
    assert_eq!(health.runtime_mode, "remote");
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );
    assert_eq!(health.v2_connections.active_connection_count, 1);
    assert_eq!(health.v2_connections.peak_active_connection_count, 1);
    assert_eq!(health.v2_connections.total_connection_count, 1);
    assert_eq!(
        health.v2_connections.last_connection_started_at.is_some(),
        true
    );
    assert_eq!(health.v2_connections.last_connection_completed_at, None);
    assert_eq!(health.v2_connections.last_connection_outcome, None);

    assert_remote_client_shutdown(connection.shutdown().await);

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

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_healthz_tracks_peak_and_total_across_multiple_v2_sessions() {
    let worker =
        start_mock_remote_server_for_idle_v2_sessions(Some("secret-token".to_string())).await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker,
                    auth_token: Some("secret-token".to_string()),
                    account_id: None,
                }],
            }),
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
    assert_eq!(first_started_health.runtime_mode, "remote");
    assert_eq!(
        first_started_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );
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

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_healthz_reports_v2_connection_activity_and_last_outcome() {
    let worker_a =
        start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a").await;
    let worker_b =
        start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b").await;
    let expected_unlabeled_account_workers = vec![
        GatewayRemoteUnlabeledAccountWorker {
            worker_id: 0,
            websocket_url: worker_a.clone(),
        },
        GatewayRemoteUnlabeledAccountWorker {
            worker_id: 1,
            websocket_url: worker_b.clone(),
        },
    ];
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
            }),
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
    assert_eq!(health.runtime_mode, "remote");
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(health.remote_account_labels_complete, Some(false));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(2));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(vec![0, 1]));
    assert_eq!(
        health.remote_unlabeled_account_workers,
        Some(expected_unlabeled_account_workers)
    );
    assert_eq!(health.v2_connections.active_connection_count, 1);
    assert_eq!(health.v2_connections.peak_active_connection_count, 1);
    assert_eq!(health.v2_connections.total_connection_count, 1);
    assert_eq!(
        health.v2_connections.last_connection_started_at.is_some(),
        true
    );
    assert_eq!(health.v2_connections.last_connection_completed_at, None);
    assert_eq!(health.v2_connections.last_connection_outcome, None);

    assert_remote_client_shutdown(connection.shutdown().await);

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

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_healthz_tracks_peak_and_total_across_multiple_v2_sessions() {
    let worker_a =
        start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a").await;
    let worker_b =
        start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b").await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
            }),
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
    assert_eq!(first_started_health.runtime_mode, "remote");
    assert_eq!(
        first_started_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
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

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_healthz_reports_slow_client_timeout_outcome() {
    let worker = start_mock_remote_server_for_slow_client_timeout().await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker,
                    auth_token: None,
                    account_id: None,
                }],
            }),
            v2_client_send_timeout: Duration::from_millis(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let health_client = reqwest::Client::new();
    let mut request = format!("ws://{}/", server.local_addr())
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
    let (mut websocket, response) = connect_async(request)
        .await
        .expect("websocket should connect");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);

    write_websocket_message(
        &mut websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "clientInfo": {
                    "name": "codex-gateway-test",
                    "version": "0.0.0-test",
                },
                "capabilities": null,
            })),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Response(_) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize response");
    };
    let JSONRPCMessage::Request(server_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded server request before slow-client timeout");
    };
    assert_eq!(server_request.method, "account/chatgptAuthTokens/refresh");

    let active_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health
                .v2_connections
                .active_connection_server_request_backlog_method_counts
                == vec![GatewayV2ServerRequestBacklogMethodCounts {
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                }]
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("active server-request backlog method health should settle");
    assert_eq!(active_health.v2_connections.active_connection_count, 1);
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_peak_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(0),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );

    let settled_health = timeout(Duration::from_secs(60), async {
        loop {
            let response = health_client
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
    assert_eq!(settled_health.runtime_mode, "remote");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(
        settled_health.v2_connections.connection_outcome_counts,
        vec![GatewayV2ConnectionOutcomeCounts {
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(settled_health.v2_connections.client_send_timeout_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_client_send_timeout_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_method
            .as_deref(),
        Some("warning")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_outcome
            .as_deref(),
        Some("client_send_timed_out")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_at
            .is_some(),
        true
    );
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
        settled_health.v2_connections.last_connection_detail,
        Some("gateway websocket send timed out".to_string())
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
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "account/chatgptAuthTokens/refresh".to_string(),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(0),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_healthz_reports_slow_client_timeout_outcome() {
    let worker_a = start_mock_remote_server_for_idle_v2_sessions(None).await;
    let worker_b = start_mock_remote_server_for_slow_client_timeout().await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
            }),
            v2_client_send_timeout: Duration::from_millis(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let health_client = reqwest::Client::new();
    let mut request = format!("ws://{}/", server.local_addr())
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
    let (mut websocket, response) = connect_async(request)
        .await
        .expect("websocket should connect");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);

    write_websocket_message(
        &mut websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "clientInfo": {
                    "name": "codex-gateway-test",
                    "version": "0.0.0-test",
                },
                "capabilities": null,
            })),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Response(_) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize response");
    };
    let JSONRPCMessage::Request(server_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded server request before slow-client timeout");
    };
    assert_eq!(server_request.method, "account/chatgptAuthTokens/refresh");

    let active_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health
                .v2_connections
                .active_connection_server_request_backlog_method_counts
                == vec![GatewayV2ServerRequestBacklogMethodCounts {
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                }]
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("active server-request backlog method health should settle");
    assert_eq!(active_health.v2_connections.active_connection_count, 1);
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_peak_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(1),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );

    let settled_health = timeout(Duration::from_secs(60), async {
        loop {
            let response = health_client
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
    assert_eq!(settled_health.runtime_mode, "remote");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(settled_health.v2_connections.client_send_timeout_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_client_send_timeout_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_method
            .as_deref(),
        Some("warning")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_outcome
            .as_deref(),
        Some("client_send_timed_out")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_at
            .is_some(),
        true
    );
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
        settled_health.v2_connections.last_connection_detail,
        Some("gateway websocket send timed out".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "account/chatgptAuthTokens/refresh".to_string(),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(1),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}

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
