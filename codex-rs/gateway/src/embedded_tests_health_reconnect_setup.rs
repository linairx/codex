use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_for_setup_mutations() {
    let (worker_a, worker_a_requests) =
        start_reconnecting_v2_multi_connection_session_mutation_server("worker-a").await;
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before same-session setup mutation test starts");

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

    sleep(Duration::from_millis(200)).await;

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
                migration_source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should reconnect and fan out");
    assert!(!imported.import_id.is_empty());

    let import_completed = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("externalAgentConfig/import/completed should finish in time")
        .expect("event stream should stay open after externalAgentConfig/import");
    assert!(matches!(
        import_completed,
        AppServerEvent::ServerNotification(ServerNotification::ExternalAgentConfigImportCompleted(
            _
        ))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate externalAgentConfig/import/completed should be suppressed after recovered import fanout"
    );

    let batch_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(2),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/shared/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite should reconnect and fan out");
    assert_eq!(batch_write.version, "worker-a");

    let config_value_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigValueWrite {
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
        })
        .await
        .expect("config/value/write should reconnect and fan out");
    assert_eq!(config_value_write.version, "worker-a");

    let marketplace: MarketplaceAddResponse = client
        .request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(4),
            params: MarketplaceAddParams {
                source: "https://example.com/shared-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/shared-plugin".to_string()]),
            },
        })
        .await
        .expect("marketplace/add should reconnect and fan out");
    assert_eq!(marketplace.marketplace_name, "shared-marketplace");

    let skills_config: SkillsConfigWriteResponse = client
        .request_typed(ClientRequest::SkillsConfigWrite {
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
        })
        .await
        .expect("skills/config/write should reconnect and fan out");
    assert_eq!(skills_config.effective_enabled, true);

    timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after skills/config/write");
            if matches!(
                event,
                AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
            ) {
                break;
            }
        }
    })
    .await
    .expect("skills/changed should arrive after recovered skills/config/write");
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate skills/changed should be suppressed after recovered skills/config/write"
    );

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(6),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        })
        .await
        .expect("experimentalFeature/enablement/set should reconnect and fan out");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let mcp_refresh: McpServerRefreshResponse = client
        .request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(7),
            params: None,
        })
        .await
        .expect("config/mcpServer/reload should reconnect and fan out");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let reset: MemoryResetResponse = client
        .request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(8),
            params: None,
        })
        .await
        .expect("memory/reset should reconnect and fan out");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = client
        .request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(9),
            params: None,
        })
        .await
        .expect("account/logout should reconnect and fan out");
    assert_eq!(logout, LogoutAccountResponse {});

    let logout_account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after account/logout");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
                && notification.auth_mode.is_none()
                && notification.plan_type.is_none()
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated should arrive after recovered account/logout fanout");
    assert_eq!(logout_account_updated.auth_mode, None);
    assert_eq!(logout_account_updated.plan_type, None);
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate account/updated should be suppressed after recovered account/logout fanout"
    );

    let watch: FsWatchResponse = client
        .request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(10),
            params: FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared-repo/.git/HEAD")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        })
        .await
        .expect("fs/watch should reconnect and fan out");
    assert_eq!(
        watch.path.as_ref().to_string_lossy(),
        "/tmp/shared-repo/.git/HEAD"
    );

    let fs_changed_paths = timeout(Duration::from_secs(5), async {
        let mut changed_paths = HashSet::new();
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                if notification.watch_id != "watch-shared" {
                    continue;
                }
                changed_paths.extend(
                    notification
                        .changed_paths
                        .into_iter()
                        .map(|path| path.as_ref().to_string_lossy().into_owned()),
                );
                if changed_paths.len() == 2 {
                    break changed_paths;
                }
            }
        }
    })
    .await
    .expect("fs/changed notifications should arrive after recovered fs/watch");
    assert_eq!(
        fs_changed_paths,
        HashSet::from([
            "/tmp/worker-a/shared-repo/.git/HEAD".to_string(),
            "/tmp/worker-b/shared-repo/.git/HEAD".to_string(),
        ])
    );

    let unwatch: FsUnwatchResponse = client
        .request_typed(ClientRequest::FsUnwatch {
            request_id: RequestId::Integer(11),
            params: FsUnwatchParams {
                watch_id: "watch-shared".to_string(),
            },
        })
        .await
        .expect("fs/unwatch should reconnect and fan out");
    assert_eq!(unwatch, FsUnwatchResponse {});

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
        "fs/watch".to_string(),
        "fs/unwatch".to_string(),
    ];
    assert_eq!(*worker_a_requests.lock().await, expected_methods);
    assert_eq!(*worker_b_requests.lock().await, expected_methods);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
