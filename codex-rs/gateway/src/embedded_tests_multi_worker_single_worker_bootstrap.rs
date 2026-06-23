use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_supports_bootstrap_refresh_requests_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_bootstrap_refresh_server().await;
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
                    websocket_url,
                    auth_token: None,
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
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && !worker.reconnecting)
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .and_then(|worker| worker.last_error.as_ref())
                    .is_some()
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");

    let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("v2 client should connect after worker reconnect");

    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time after worker reconnect")
    .expect("account/read should succeed after worker reconnect");
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::Chatgpt {
                email: Some("gateway@example.com".to_string()),
                plan_type: AccountPlanType::Pro,
            }),
            requires_openai_auth: false,
        }
    );

    let codex_rate_limit = RateLimitSnapshot {
        limit_id: Some("codex".to_string()),
        limit_name: Some("Codex".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 42,
            window_duration_mins: Some(300),
            resets_at: Some(1_700_000_000),
        }),
        secondary: None,
        credits: None,
        plan_type: Some(AccountPlanType::Pro),
        rate_limit_reached_type: None,
        individual_limit: None,
    };
    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(2),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time after worker reconnect")
    .expect("account/rateLimits/read should succeed after worker reconnect");
    assert_eq!(
        rate_limits,
        GetAccountRateLimitsResponse {
            rate_limits: codex_rate_limit.clone(),
            rate_limits_by_limit_id: Some(
                [("codex".to_string(), codex_rate_limit)]
                    .into_iter()
                    .collect(),
            ),
            rate_limit_reset_credits: None,
        }
    );

    let models: ModelListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(3),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list should finish in time after worker reconnect")
    .expect("model/list should succeed after worker reconnect");
    assert_eq!(models.next_cursor, None);
    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "openai/gpt-5");

    let external_agent_config: ExternalAgentConfigDetectResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(4),
            params: ExternalAgentConfigDetectParams {
                include_home: true,
                cwds: Some(vec![PathBuf::from("/tmp/reconnected-project")]),
            },
        }),
    )
    .await
    .expect("externalAgentConfig/detect should finish in time after worker reconnect")
    .expect("externalAgentConfig/detect should succeed after worker reconnect");
    assert_eq!(external_agent_config.items.len(), 1);
    assert_eq!(
        external_agent_config.items[0].description,
        "reconnected repo config"
    );

    let apps: AppsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(5),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        }),
    )
    .await
    .expect("app/list should finish in time after worker reconnect")
    .expect("app/list should succeed after worker reconnect");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "reconnected-app");

    let skills: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(6),
            params: SkillsListParams {
                cwds: vec![PathBuf::from("/tmp/reconnected-project")],
                force_reload: false,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after worker reconnect")
    .expect("skills/list should succeed after worker reconnect");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(
        skills.data[0].cwd,
        PathBuf::from("/tmp/reconnected-project")
    );
    assert_eq!(skills.data[0].skills.len(), 1);
    assert_eq!(skills.data[0].skills[0].name, "reconnected-skill");

    let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
        "cwds": ["/tmp/reconnected-project"],
    }))
    .expect("plugin/list params should deserialize");
    let plugins: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(7),
            params: plugin_list_params,
        }),
    )
    .await
    .expect("plugin/list should finish in time after worker reconnect")
    .expect("plugin/list should succeed after worker reconnect");
    assert_eq!(plugins.marketplace_load_errors, Vec::new());
    assert_eq!(plugins.marketplaces.len(), 1);
    assert_eq!(plugins.marketplaces[0].name, "reconnected-marketplace");
    assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
    assert_eq!(plugins.marketplaces[0].plugins[0].id, "reconnected-plugin");
    assert_eq!(
        plugins.featured_plugin_ids,
        vec!["reconnected-plugin".to_string()]
    );

    let plugin: PluginReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(8),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/reconnected-project/marketplace.json",
                "pluginName": "reconnected-plugin",
            }))
            .expect("plugin/read params should deserialize"),
        }),
    )
    .await
    .expect("plugin/read should finish in time after worker reconnect")
    .expect("plugin/read should succeed after worker reconnect");
    assert_eq!(plugin.plugin.summary.name, "reconnected-plugin");
    assert_eq!(plugin.plugin.marketplace_name, "reconnected-marketplace");
    assert_eq!(plugin.plugin.skills.len(), 1);
    assert_eq!(plugin.plugin.skills[0].name, "reconnected-skill");

    let install: PluginInstallResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(9),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/reconnected-project/marketplace.json",
                "pluginName": "reconnected-plugin",
            }))
            .expect("plugin/install params should deserialize"),
        }),
    )
    .await
    .expect("plugin/install should finish in time after worker reconnect")
    .expect("plugin/install should succeed after worker reconnect");
    assert_eq!(
        install,
        PluginInstallResponse {
            auth_policy: PluginAuthPolicy::OnInstall,
            apps_needing_auth: Vec::new(),
        }
    );

    let plugins_after_install: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(10),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/reconnected-project"],
            }))
            .expect("plugin/list params after install should deserialize"),
        }),
    )
    .await
    .expect("plugin/list after install should finish in time after worker reconnect")
    .expect("plugin/list after install should succeed after worker reconnect");
    assert_eq!(
        plugins_after_install.marketplaces[0].plugins[0].installed,
        true
    );

    let uninstall: PluginUninstallResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(11),
            params: PluginUninstallParams {
                plugin_id: "reconnected-plugin@reconnected-marketplace".to_string(),
            },
        }),
    )
    .await
    .expect("plugin/uninstall should finish in time after worker reconnect")
    .expect("plugin/uninstall should succeed after worker reconnect");
    assert_eq!(uninstall, PluginUninstallResponse {});

    let plugins_after_uninstall: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(12),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/reconnected-project"],
            }))
            .expect("plugin/list params after uninstall should deserialize"),
        }),
    )
    .await
    .expect("plugin/list after uninstall should finish in time after worker reconnect")
    .expect("plugin/list after uninstall should succeed after worker reconnect");
    assert_eq!(
        plugins_after_uninstall.marketplaces[0].plugins[0].installed,
        false
    );

    let config_read: ConfigReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(13),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/reconnected-project".to_string()),
            },
        }),
    )
    .await
    .expect("config/read should finish in time after worker reconnect")
    .expect("config/read should succeed after worker reconnect");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5"));
    assert_eq!(config_read.layers.is_some(), true);

    let config_requirements: ConfigRequirementsReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(14),
            params: None,
        }),
    )
    .await
    .expect("configRequirements/read should finish in time after worker reconnect")
    .expect("configRequirements/read should succeed after worker reconnect");
    assert_eq!(config_requirements.requirements, None);

    let experimental_features: ExperimentalFeatureListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(15),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(10),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("experimentalFeature/list should finish in time after worker reconnect")
    .expect("experimentalFeature/list should succeed after worker reconnect");
    assert_eq!(experimental_features.next_cursor, None);
    assert_eq!(experimental_features.data.len(), 1);
    assert_eq!(experimental_features.data[0].name, "gateway-test-feature");

    let collaboration_modes: CollaborationModeListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(16),
            params: CollaborationModeListParams::default(),
        }),
    )
    .await
    .expect("collaborationMode/list should finish in time after worker reconnect")
    .expect("collaborationMode/list should succeed after worker reconnect");
    assert_eq!(collaboration_modes.data.len(), 1);
    assert_eq!(collaboration_modes.data[0].name, "default");

    let mcp_statuses: ListMcpServerStatusResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(17),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: None,
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServerStatus/list should finish in time after worker reconnect")
    .expect("mcpServerStatus/list should succeed after worker reconnect");
    assert_eq!(mcp_statuses.data.len(), 1);
    assert_eq!(mcp_statuses.data[0].name, "reconnected-mcp");

    let imported: ExternalAgentConfigImportResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(18),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/import should finish in time after worker reconnect")
    .expect("externalAgentConfig/import should succeed after worker reconnect");
    assert!(!imported.import_id.is_empty());

    let marketplace: MarketplaceAddResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(19),
            params: MarketplaceAddParams {
                source: "https://example.com/reconnected-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/reconnected-plugin".to_string()]),
            },
        }),
    )
    .await
    .expect("marketplace/add should finish in time after worker reconnect")
    .expect("marketplace/add should succeed after worker reconnect");
    assert_eq!(marketplace.marketplace_name, "reconnected-marketplace");
    assert_eq!(marketplace.already_added, false);
    assert_eq!(
        marketplace.installed_root.as_path(),
        PathBuf::from("/tmp/reconnected-project/marketplace").as_path()
    );

    let skills_config: SkillsConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(20),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/reconnected-project/.codex/skills/reconnected-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        }),
    )
    .await
    .expect("skills/config/write should finish in time after worker reconnect")
    .expect("skills/config/write should succeed after worker reconnect");
    assert_eq!(skills_config.effective_enabled, true);

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(21),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        }),
    )
    .await
    .expect("experimentalFeature/enablement/set should finish in time after worker reconnect")
    .expect("experimentalFeature/enablement/set should succeed after worker reconnect");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let mcp_refresh: McpServerRefreshResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(22),
            params: None,
        }),
    )
    .await
    .expect("config/mcpServer/reload should finish in time after worker reconnect")
    .expect("config/mcpServer/reload should succeed after worker reconnect");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let batch_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(23),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/reconnected-project/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        }),
    )
    .await
    .expect("config/batchWrite should finish in time after worker reconnect")
    .expect("config/batchWrite should succeed after worker reconnect");
    assert_eq!(batch_write.status, WriteStatus::Ok);
    assert_eq!(batch_write.version, "reconnected-version-1");
    assert_eq!(
        batch_write.file_path.as_path(),
        PathBuf::from("/tmp/reconnected-project/config.toml").as_path()
    );
    assert_eq!(batch_write.overridden_metadata, None);

    let config_value_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(24),
            params: ConfigValueWriteParams {
                key_path: "plugins.reconnected-plugin".to_string(),
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
    .expect("config/value/write should finish in time after worker reconnect")
    .expect("config/value/write should succeed after worker reconnect");
    assert_eq!(config_value_write.status, WriteStatus::Ok);
    assert_eq!(config_value_write.version, "reconnected-version-1");
    assert_eq!(
        config_value_write.file_path.as_path(),
        PathBuf::from("/tmp/reconnected-project/config.toml").as_path()
    );
    assert_eq!(config_value_write.overridden_metadata, None);

    let reset: MemoryResetResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(25),
            params: None,
        }),
    )
    .await
    .expect("memory/reset should finish in time after worker reconnect")
    .expect("memory/reset should succeed after worker reconnect");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(26),
            params: None,
        }),
    )
    .await
    .expect("account/logout should finish in time after worker reconnect")
    .expect("account/logout should succeed after worker reconnect");
    assert_eq!(logout, LogoutAccountResponse {});

    let feedback: FeedbackUploadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(27),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway single-worker recovery".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        }),
    )
    .await
    .expect("feedback/upload should finish in time after worker reconnect")
    .expect("feedback/upload should succeed after worker reconnect");
    assert_eq!(feedback.thread_id, "feedback-thread-reconnected");

    let add_credits: SendAddCreditsNudgeEmailResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(28),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        }),
    )
    .await
    .expect("account/sendAddCreditsNudgeEmail should finish in time after worker reconnect")
    .expect("account/sendAddCreditsNudgeEmail should succeed after worker reconnect");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let create_directory: FsCreateDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(29),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        }),
    )
    .await
    .expect("fs/createDirectory should finish in time after worker reconnect")
    .expect("fs/createDirectory should succeed after worker reconnect");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(30),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVjb25uZWN0ZWQtZmlsZQ==".to_string(),
            },
        }),
    )
    .await
    .expect("fs/writeFile should finish in time after worker reconnect")
    .expect("fs/writeFile should succeed after worker reconnect");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(31),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readFile should finish in time after worker reconnect")
    .expect("fs/readFile should succeed after worker reconnect");
    assert_eq!(read_file.data_base64, "cmVjb25uZWN0ZWQtZmlsZQ==");

    let metadata: FsGetMetadataResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(32),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/getMetadata should finish in time after worker reconnect")
    .expect("fs/getMetadata should succeed after worker reconnect");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(33),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readDirectory should finish in time after worker reconnect")
    .expect("fs/readDirectory should succeed after worker reconnect");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");

    let copy: FsCopyResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(34),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        }),
    )
    .await
    .expect("fs/copy should finish in time after worker reconnect")
    .expect("fs/copy should succeed after worker reconnect");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(35),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(false),
                force: Some(true),
            },
        }),
    )
    .await
    .expect("fs/remove should finish in time after worker reconnect")
    .expect("fs/remove should succeed after worker reconnect");
    assert_eq!(remove, FsRemoveResponse {});

    let watch: FsWatchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(36),
            params: FsWatchParams {
                watch_id: "reconnected-watch".to_string(),
                path: PathBuf::from("/tmp/reconnected-project/config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/watch should finish in time after worker reconnect")
    .expect("fs/watch should succeed after worker reconnect");
    assert_eq!(
        watch.path.as_ref().to_string_lossy(),
        "/tmp/reconnected-project/config.toml"
    );

    let fs_changed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed should arrive after recovered fs/watch");
    assert_eq!(fs_changed.watch_id, "reconnected-watch");
    assert_eq!(
        fs_changed
            .changed_paths
            .iter()
            .map(|path| path.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec!["/tmp/reconnected-project/config.toml".to_string()]
    );

    let fuzzy_search: FuzzyFileSearchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(37),
            params: FuzzyFileSearchParams {
                query: "reconnect".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
                cancellation_token: Some("reconnected-fuzzy-search".to_string()),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch should finish in time after worker reconnect")
    .expect("fuzzyFileSearch should succeed after worker reconnect");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].root, "/tmp/reconnected-project");
    assert_eq!(fuzzy_search.files[0].path, "docs/reconnected.md");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(38),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStart should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStart should succeed after worker reconnect");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(39),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                query: "reconnect".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionUpdate should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionUpdate should succeed after worker reconnect");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy update");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
                && notification.query == "reconnect"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated should arrive after worker reconnect");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(
        fuzzy_session_updated.files[0].root,
        "/tmp/reconnected-project"
    );
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/reconnected.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(40),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "reconnected-fuzzy-session".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStop should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStop should succeed after worker reconnect");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy stop");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted should arrive after worker reconnect");
    assert_eq!(
        fuzzy_session_completed.session_id,
        "reconnected-fuzzy-session"
    );

    let windows_setup: WindowsSandboxSetupStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(41),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/reconnected-project")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        }),
    )
    .await
    .expect("windowsSandbox/setupStart should finish in time after worker reconnect")
    .expect("windowsSandbox/setupStart should succeed after worker reconnect");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered windows setup");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted should arrive after worker reconnect");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(42),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf remote-command".to_string(),
                ],
                process_id: Some("proc-remote".to_string()),
                tty: true,
                stream_stdin: true,
                stream_stdout_stderr: true,
                output_bytes_cap: None,
                disable_output_cap: false,
                disable_timeout: false,
                timeout_ms: None,
                cwd: None,
                env: None,
                size: Some(CommandExecTerminalSize { rows: 24, cols: 80 }),
                sandbox_policy: None,
                permission_profile: None,
            },
        }),
    )
    .await
    .expect("command/exec should finish in time after worker reconnect")
    .expect("command/exec should succeed after worker reconnect");
    assert_eq!(
        command_exec,
        CommandExecResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    );

    let command_output = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered command/exec");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
                && notification.process_id == "proc-remote"
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta should arrive after worker reconnect");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "cmVtb3RlLWNvbW1hbmQ=");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(43),
            params: CommandExecWriteParams {
                process_id: "proc-remote".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        }),
    )
    .await
    .expect("command/exec/write should finish in time after worker reconnect")
    .expect("command/exec/write should succeed after worker reconnect");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(44),
            params: CommandExecResizeParams {
                process_id: "proc-remote".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        }),
    )
    .await
    .expect("command/exec/resize should finish in time after worker reconnect")
    .expect("command/exec/resize should succeed after worker reconnect");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(45),
            params: CommandExecTerminateParams {
                process_id: "proc-remote".to_string(),
            },
        }),
    )
    .await
    .expect("command/exec/terminate should finish in time after worker reconnect")
    .expect("command/exec/terminate should succeed after worker reconnect");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    let mcp_oauth_login: McpServerOauthLoginResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(46),
            params: McpServerOauthLoginParams {
                name: "reconnected-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
            },
        }),
    )
    .await
    .expect("mcpServer/oauth/login should finish in time after worker reconnect")
    .expect("mcpServer/oauth/login should succeed after worker reconnect");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/reconnected-mcp"
    );

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::McpServerOauthLoginCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/oauthLogin/completed should arrive after worker reconnect");
    assert_eq!(mcp_oauth_completed.name, "reconnected-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), v2_client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
