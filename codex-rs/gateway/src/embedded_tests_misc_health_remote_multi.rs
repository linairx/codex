use super::super::super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_healthz_reports_worker_pool_from_runtime_state() {
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
                        websocket_url: worker_a.clone(),
                        auth_token: None,
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b.clone(),
                        auth_token: None,
                        account_id: Some("acct-b".to_string()),
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

    let http_client = reqwest::Client::new();
    let initial_health = http_client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response")
        .json::<GatewayHealthResponse>()
        .await
        .expect("health body");
    let initial_worker_pool = initial_health
        .worker_pool
        .expect("remote health should include worker pool");
    assert_eq!(
        initial_worker_pool,
        GatewayWorkerPoolSnapshot {
            account_count: 2,
            available_account_count: 0,
            leased_account_count: 2,
            cooldown_account_count: 0,
            policy_eligible_account_count: 2,
            policy_ineligible_account_count: 0,
            worker_slot_count: 2,
            bound_worker_slot_count: 2,
            healthy_worker_slot_count: 2,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![
                GatewayAccountPoolEntry {
                    account_id: "acct-a".to_string(),
                    account_login_state_path: None,
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(0),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
                GatewayAccountPoolEntry {
                    account_id: "acct-b".to_string(),
                    account_login_state_path: None,
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(1),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
            ],
            worker_slots: vec![
                GatewayWorkerPoolSlot {
                    worker_id: 0,
                    websocket_url: worker_a.clone(),
                    account_id: Some("acct-a".to_string()),
                    account_login_state_path: None,
                    account_capacity: Some(GatewayAccountCapacityStatus::Available),
                    account_capacity_reason: None,
                    healthy: Some(true),
                    reconnecting: Some(false),
                    last_error: None,
                },
                GatewayWorkerPoolSlot {
                    worker_id: 1,
                    websocket_url: worker_b.clone(),
                    account_id: Some("acct-b".to_string()),
                    account_login_state_path: None,
                    account_capacity: Some(GatewayAccountCapacityStatus::Available),
                    account_capacity_reason: None,
                    healthy: Some(true),
                    reconnecting: Some(false),
                    last_error: None,
                },
            ],
        }
    );

    let client = RemoteAppServerClient::connect_with_headers(
        RemoteAppServerConnectArgs {
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
        },
        vec![
            ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
            ("x-codex-project-id".to_string(), "project-a".to_string()),
        ],
    )
    .await
    .expect("client should connect to multi-worker gateway");

    let _started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                cwd: Some("/tmp/project-a".to_string()),
                ephemeral: Some(true),
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should register project route");

    let routed_health = http_client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response")
        .json::<GatewayHealthResponse>()
        .await
        .expect("health body");
    let routed_worker_pool = routed_health
        .worker_pool
        .expect("remote health should include worker pool");
    assert_eq!(
        routed_worker_pool,
        GatewayWorkerPoolSnapshot {
            account_count: 2,
            available_account_count: 0,
            leased_account_count: 2,
            cooldown_account_count: 0,
            policy_eligible_account_count: 2,
            policy_ineligible_account_count: 0,
            worker_slot_count: 2,
            bound_worker_slot_count: 2,
            healthy_worker_slot_count: 2,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![
                GatewayAccountPoolEntry {
                    account_id: "acct-a".to_string(),
                    account_login_state_path: None,
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(0),
                    project_route_count: 1,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
                GatewayAccountPoolEntry {
                    account_id: "acct-b".to_string(),
                    account_login_state_path: None,
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(1),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
            ],
            worker_slots: vec![
                GatewayWorkerPoolSlot {
                    worker_id: 0,
                    websocket_url: worker_a,
                    account_id: Some("acct-a".to_string()),
                    account_login_state_path: None,
                    account_capacity: Some(GatewayAccountCapacityStatus::Available),
                    account_capacity_reason: None,
                    healthy: Some(true),
                    reconnecting: Some(false),
                    last_error: None,
                },
                GatewayWorkerPoolSlot {
                    worker_id: 1,
                    websocket_url: worker_b,
                    account_id: Some("acct-b".to_string()),
                    account_login_state_path: None,
                    account_capacity: Some(GatewayAccountCapacityStatus::Available),
                    account_capacity_reason: None,
                    healthy: Some(true),
                    reconnecting: Some(false),
                    last_error: None,
                },
            ],
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
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
