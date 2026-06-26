use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_routes_config_read_by_cwd_and_aggregates_capability_requests_over_v2()
{
    let worker_a = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-a",
            requires_openai_auth: false,
            rate_limits: vec![("codex", "Codex", 20)],
            models: vec![("shared-model", "Shared Model", true)],
            apps: vec![("shared-app", "Shared App")],
            mcp_status_names: vec!["shared-mcp"],
            shared_cwd: "/tmp/shared-repo",
            unique_cwd: "/tmp/worker-a-only",
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-b",
            requires_openai_auth: true,
            rate_limits: vec![("worker-b", "Worker B", 35)],
            models: vec![("worker-b-model", "Worker B Model", true)],
            apps: vec![("worker-b-app", "Worker B App")],
            mcp_status_names: vec!["worker-b-mcp"],
            shared_cwd: "/tmp/shared-repo",
            unique_cwd: "/tmp/worker-b-only",
        },
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

    let config_read: ConfigReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(1),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/worker-b-only/subdir".to_string()),
            },
        }),
    )
    .await
    .expect("config/read should finish in time")
    .expect("config/read should succeed through multi-worker gateway");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5-worker-b"));
    assert_eq!(config_read.layers.is_some(), true);

    let shared_config_read: ConfigReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(2),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/shared-repo".to_string()),
            },
        }),
    )
    .await
    .expect("shared config/read should finish in time")
    .expect("shared config/read should succeed through multi-worker gateway");
    assert_eq!(
        shared_config_read.config.model.as_deref(),
        Some("gpt-5-worker-a")
    );
    assert_eq!(shared_config_read.layers.is_some(), true);

    let config_requirements: ConfigRequirementsReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("configRequirements/read should finish in time")
    .expect("configRequirements/read should succeed through multi-worker gateway");
    assert_eq!(config_requirements.requirements, None);

    let experimental_features: ExperimentalFeatureListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(4),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(20),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("experimentalFeature/list should finish in time")
    .expect("experimentalFeature/list should succeed through multi-worker gateway");
    assert_eq!(
        experimental_features
            .data
            .iter()
            .map(|feature| feature.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "gateway-test-feature-worker-a",
            "gateway-test-feature-worker-b",
        ]
    );
    assert_eq!(experimental_features.next_cursor, None);

    let collaboration_modes: CollaborationModeListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(5),
            params: CollaborationModeListParams::default(),
        }),
    )
    .await
    .expect("collaborationMode/list should finish in time")
    .expect("collaborationMode/list should succeed through multi-worker gateway");
    assert_eq!(
        collaboration_modes
            .data
            .iter()
            .map(|mode| mode.name.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-a-default", "worker-b-default"]
    );

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_paginates_experimental_feature_discovery_over_v2() {
    let worker_a = start_mock_remote_multi_connection_experimental_feature_list_server(vec![
        "worker-a-first",
        "shared-feature",
    ])
    .await;
    let worker_b = start_mock_remote_multi_connection_experimental_feature_list_server(vec![
        "worker-b-first",
        "shared-feature",
    ])
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

    let first_page: ExperimentalFeatureListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(1),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(2),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("experimentalFeature/list first page should finish in time")
    .expect("experimentalFeature/list first page should aggregate through gateway");
    assert_eq!(
        first_page.next_cursor.as_deref(),
        Some("experimental-feature-offset:2")
    );
    assert_eq!(
        first_page
            .data
            .iter()
            .map(|feature| feature.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-feature", "worker-a-first"]
    );

    let second_page: ExperimentalFeatureListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(2),
            params: ExperimentalFeatureListParams {
                cursor: first_page.next_cursor,
                limit: Some(2),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("experimentalFeature/list second page should finish in time")
    .expect("experimentalFeature/list second page should aggregate through gateway");
    assert_eq!(second_page.next_cursor, None);
    assert_eq!(
        second_page
            .data
            .iter()
            .map(|feature| feature.name.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-b-first"]
    );

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
