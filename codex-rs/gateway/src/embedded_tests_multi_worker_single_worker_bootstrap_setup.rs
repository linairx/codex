use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_refresh_setup_requests(v2_client: &mut RemoteAppServerClient) {
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
}
