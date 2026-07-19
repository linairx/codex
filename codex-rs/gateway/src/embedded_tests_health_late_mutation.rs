use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_deduplicates_external_agent_import_completed_notifications_over_v2() {
    let (worker_a, _worker_a_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-a").await;
    let (worker_b, _worker_b_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-b").await;
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

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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

    let imported: ExternalAgentConfigImportResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
                migration_source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/import should finish in time")
    .expect("externalAgentConfig/import should fan out through multi-worker gateway");
    assert!(!imported.import_id.is_empty());

    let notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("externalAgentConfig/import/completed should finish in time")
        .expect("event stream should stay open");
    assert!(matches!(
        notification,
        AppServerEvent::ServerNotification(ServerNotification::ExternalAgentConfigImportCompleted(
            _
        ))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate externalAgentConfig/import/completed should be suppressed"
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
async fn remote_multi_worker_supports_drop_in_v2_client_setup_mutation_workflow() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-a").await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-b").await;
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

    let imported: ExternalAgentConfigImportResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
                migration_source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/import should finish in time")
    .expect("externalAgentConfig/import should fan out through multi-worker gateway");
    assert!(!imported.import_id.is_empty());

    let batch_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(2),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/shared/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        }),
    )
    .await
    .expect("config/batchWrite should finish in time")
    .expect("config/batchWrite should fan out through multi-worker gateway");
    assert_eq!(batch_write.version, "worker-a");

    let config_value_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(3),
            params: ConfigValueWriteParams {
                key_path: "plugins.shared-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        }),
    )
    .await
    .expect("config/value/write should finish in time")
    .expect("config/value/write should fan out through multi-worker gateway");
    assert_eq!(config_value_write.version, "worker-a");

    let marketplace: MarketplaceAddResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(4),
            params: MarketplaceAddParams {
                source: "https://example.com/shared-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/shared-plugin".to_string()]),
            },
        }),
    )
    .await
    .expect("marketplace/add should finish in time")
    .expect("marketplace/add should fan out through multi-worker gateway");
    assert_eq!(marketplace.marketplace_name, "shared-marketplace");

    let skills_config: SkillsConfigWriteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(5),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/shared/skills/shared-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        }),
    )
    .await
    .expect("skills/config/write should finish in time")
    .expect("skills/config/write should fan out through multi-worker gateway");
    assert_eq!(skills_config.effective_enabled, true);

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(6),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        }),
    )
    .await
    .expect("experimentalFeature/enablement/set should finish in time")
    .expect("experimentalFeature/enablement/set should fan out through multi-worker gateway");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let mcp_refresh: McpServerRefreshResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(7),
            params: None,
        }),
    )
    .await
    .expect("config/mcpServer/reload should finish in time")
    .expect("config/mcpServer/reload should fan out through multi-worker gateway");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let reset: MemoryResetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(8),
            params: None,
        }),
    )
    .await
    .expect("memory/reset should finish in time")
    .expect("memory/reset should fan out through multi-worker gateway");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(9),
            params: None,
        }),
    )
    .await
    .expect("account/logout should finish in time")
    .expect("account/logout should fan out through multi-worker gateway");
    assert_eq!(logout, LogoutAccountResponse {});

    let expected_methods = vec![
        "externalAgentConfig/import".to_string(),
        "config/batchWrite".to_string(),
        "config/value/write".to_string(),
        "marketplace/add".to_string(),
        "skills/config/write".to_string(),
        "experimentalFeature/enablement/set".to_string(),
        "config/mcpServer/reload".to_string(),
        "memory/reset".to_string(),
        "account/logout".to_string(),
    ];
    assert_eq!(*worker_a_requests.lock().await, expected_methods);
    assert_eq!(*worker_b_requests.lock().await, expected_methods);

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
