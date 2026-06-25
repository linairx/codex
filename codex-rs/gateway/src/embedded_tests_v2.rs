use super::*;

macro_rules! thread_handoff_fail_closed_test_setup {
    () => {{
        let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
            "worker-a",
            WorkerAAccountCapacity::Exhausted,
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_legacy_account_handoff_server(
            "worker-b",
            WorkerAAccountCapacity::Available,
        )
        .await;
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
                            account_id: Some("acct-a".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
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

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
        .expect("remote client should connect to multi-worker gateway");

        let _worker_a_thread: AppServerThreadStartResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/worker-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    ..Default::default()
                },
            }),
        )
        .await
        .expect("first thread/start should finish in time")
        .expect("first thread/start should register worker A scope");

        let worker_b_thread: AppServerThreadStartResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/worker-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    ..Default::default()
                },
            }),
        )
        .await
        .expect("second thread/start should finish in time")
        .expect("second thread/start should register worker B scope");

        timeout(
            Duration::from_secs(5),
            client.request_typed::<GetAccountRateLimitsResponse>(
                ClientRequest::GetAccountRateLimits {
                    request_id: RequestId::Integer(3),
                    params: None,
                },
            ),
        )
        .await
        .expect("account/rateLimits/read should finish in time")
        .expect("account/rateLimits/read should mark every worker exhausted");

        (server, client, worker_b_thread)
    }};
}

#[path = "embedded_tests_v2_restore.rs"]
mod embedded_tests_v2_restore;

#[path = "embedded_tests_v2_restore_fail_closed.rs"]
mod embedded_tests_v2_restore_fail_closed;

#[path = "embedded_tests_v2_restore_legacy_summary.rs"]
mod embedded_tests_v2_restore_legacy_summary;

#[path = "embedded_tests_v2_late.rs"]
mod embedded_tests_v2_late;

#[path = "embedded_tests_v2_path_handoff_fail_closed.rs"]
mod embedded_tests_v2_path_handoff_fail_closed;

#[path = "embedded_tests_v2_thread_handoff_fail_closed.rs"]
mod embedded_tests_v2_thread_handoff_fail_closed;

#[path = "embedded_tests_v2_thread_handoff_fail_closed_tail.rs"]
mod embedded_tests_v2_thread_handoff_fail_closed_tail;

#[path = "embedded_tests_v2_turn_handoff_fail_closed.rs"]
mod embedded_tests_v2_turn_handoff_fail_closed;
