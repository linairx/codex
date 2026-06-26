use super::super::super::*;
use pretty_assertions::assert_eq;
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
