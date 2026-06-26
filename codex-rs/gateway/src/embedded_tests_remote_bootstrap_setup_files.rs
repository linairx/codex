use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_setup_plugins_files_and_search(
    client: &mut RemoteAppServerClient,
) {
    let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
        "cwds": ["/tmp/remote-project"],
    }))
    .expect("plugin/list params should deserialize");
    let plugins: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(12),
            params: plugin_list_params,
        })
        .await
        .expect("plugin/list should succeed through remote gateway");
    assert_eq!(plugins.marketplaces.len(), 1);
    assert_eq!(plugins.marketplaces[0].name, "remote-marketplace");
    assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
    assert_eq!(plugins.marketplaces[0].plugins[0].name, "remote-plugin");
    assert_eq!(plugins.marketplaces[0].plugins[0].installed, false);

    let marketplace: MarketplaceAddResponse = client
        .request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(46),
            params: MarketplaceAddParams {
                source: "https://example.com/remote-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/remote-plugin".to_string()]),
            },
        })
        .await
        .expect("marketplace/add should succeed through remote gateway");
    assert_eq!(marketplace.marketplace_name, "remote-marketplace");
    assert_eq!(marketplace.already_added, false);
    assert_eq!(
        marketplace.installed_root.as_path(),
        PathBuf::from("/tmp/remote-project/marketplace").as_path()
    );

    let plugin_read_params: PluginReadParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": "/tmp/remote-project/marketplace.json",
        "pluginName": "remote-plugin",
    }))
    .expect("plugin/read params should deserialize");
    let plugin: PluginReadResponse = client
        .request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(13),
            params: plugin_read_params,
        })
        .await
        .expect("plugin/read should succeed through remote gateway");
    assert_eq!(plugin.plugin.summary.name, "remote-plugin");
    assert_eq!(plugin.plugin.marketplace_name, "remote-marketplace");
    assert_eq!(plugin.plugin.skills.len(), 1);
    assert_eq!(plugin.plugin.skills[0].name, "remote-skill");

    let plugin_install_params: PluginInstallParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": "/tmp/remote-project/marketplace.json",
        "pluginName": "remote-plugin",
    }))
    .expect("plugin/install params should deserialize");
    let install: PluginInstallResponse = client
        .request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(14),
            params: plugin_install_params,
        })
        .await
        .expect("plugin/install should succeed through remote gateway");
    assert_eq!(
        install,
        PluginInstallResponse {
            auth_policy: PluginAuthPolicy::OnInstall,
            apps_needing_auth: Vec::new(),
        }
    );

    let plugins_after_install: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(15),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/remote-project"],
            }))
            .expect("plugin/list params after install should deserialize"),
        })
        .await
        .expect("plugin/list after install should succeed through remote gateway");
    assert_eq!(
        plugins_after_install.marketplaces[0].plugins[0].installed,
        true
    );

    let uninstall: PluginUninstallResponse = client
        .request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(16),
            params: PluginUninstallParams {
                plugin_id: "remote-plugin@remote-marketplace".to_string(),
            },
        })
        .await
        .expect("plugin/uninstall should succeed through remote gateway");
    assert_eq!(uninstall, PluginUninstallResponse {});

    let plugins_after_uninstall: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(17),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/remote-project"],
            }))
            .expect("plugin/list params after uninstall should deserialize"),
        })
        .await
        .expect("plugin/list after uninstall should succeed through remote gateway");
    assert_eq!(
        plugins_after_uninstall.marketplaces[0].plugins[0].installed,
        false
    );

    let batch_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(18),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/remote-project/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite should succeed through remote gateway");
    assert_eq!(batch_write.status, WriteStatus::Ok);
    assert_eq!(batch_write.version, "remote-version-1");
    assert_eq!(
        batch_write.file_path.as_path(),
        PathBuf::from("/tmp/remote-project/config.toml").as_path()
    );
    assert_eq!(batch_write.overridden_metadata, None);

    let config_value_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(19),
            params: ConfigValueWriteParams {
                key_path: "plugins.remote-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        })
        .await
        .expect("config/value/write should succeed through remote gateway");
    assert_eq!(config_value_write.status, WriteStatus::Ok);

    let feature_enablement_get: ExperimentalFeatureEnablementSetResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(20),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        })
        .await
        .expect("experimentalFeature/enablement/set should succeed through remote gateway");
    assert_eq!(
        feature_enablement_get.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let watch: FsWatchResponse = client
        .request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(21),
            params: FsWatchParams {
                watch_id: "remote-watch".to_string(),
                path: PathBuf::from("/tmp/remote-project/config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        })
        .await
        .expect("fs/watch should succeed through remote gateway");
    assert_eq!(
        watch.path.as_ref(),
        PathBuf::from("/tmp/remote-project/config.toml").as_path()
    );

    let fs_changed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed notification should arrive after fs/watch");
    assert_eq!(fs_changed.watch_id, "remote-watch");
    assert_eq!(
        fs_changed
            .changed_paths
            .iter()
            .map(|path| path.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec!["/tmp/remote-project/config.toml".to_string()]
    );

    let unwatch: FsUnwatchResponse = client
        .request_typed(ClientRequest::FsUnwatch {
            request_id: RequestId::Integer(21),
            params: FsUnwatchParams {
                watch_id: "remote-watch".to_string(),
            },
        })
        .await
        .expect("fs/unwatch should succeed through remote gateway");
    assert_eq!(unwatch, FsUnwatchResponse {});

    let create_directory: FsCreateDirectoryResponse = client
        .request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(33),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/remote-project/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        })
        .await
        .expect("fs/createDirectory should succeed through remote gateway");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = client
        .request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(34),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVtb3RlLWZpbGU=".to_string(),
            },
        })
        .await
        .expect("fs/writeFile should succeed through remote gateway");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = client
        .request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(35),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        })
        .await
        .expect("fs/readFile should succeed through remote gateway");
    assert_eq!(read_file.data_base64, "cmVtb3RlLWZpbGU=");

    let metadata: FsGetMetadataResponse = client
        .request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(36),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        })
        .await
        .expect("fs/getMetadata should succeed through remote gateway");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = client
        .request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(37),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/remote-project/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        })
        .await
        .expect("fs/readDirectory should succeed through remote gateway");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");
    assert_eq!(directory.entries[0].is_file, true);

    let copy: FsCopyResponse = client
        .request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(38),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/remote-project/nested/gateway-copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        })
        .await
        .expect("fs/copy should succeed through remote gateway");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = client
        .request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(39),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway-copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(true),
                force: Some(true),
            },
        })
        .await
        .expect("fs/remove should succeed through remote gateway");
    assert_eq!(remove, FsRemoveResponse {});

    let fuzzy_search: FuzzyFileSearchResponse = client
        .request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(29),
            params: FuzzyFileSearchParams {
                query: "gate".to_string(),
                roots: vec!["/tmp/remote-project".to_string()],
                cancellation_token: Some("remote-fuzzy-search".to_string()),
            },
        })
        .await
        .expect("fuzzyFileSearch should succeed through remote gateway");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].root, "/tmp/remote-project");
    assert_eq!(fuzzy_search.files[0].path, "docs/gateway.md");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let diff: GitDiffToRemoteResponse = client
        .request_typed(ClientRequest::GitDiffToRemote {
            request_id: RequestId::Integer(50),
            params: GitDiffToRemoteParams {
                cwd: PathBuf::from("/tmp/remote-project"),
            },
        })
        .await
        .expect("gitDiffToRemote should succeed through remote gateway");
    assert_eq!(diff.sha.0, "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(
        diff.diff,
        "diff --git a/docs/gateway.md b/docs/gateway.md\n"
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(30),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "remote-fuzzy-session".to_string(),
                roots: vec!["/tmp/remote-project".to_string()],
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStart should succeed through remote gateway");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(31),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "remote-fuzzy-session".to_string(),
                query: "gate".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionUpdate should succeed through remote gateway");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "remote-fuzzy-session"
                && notification.query == "gate"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated notification should arrive");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(fuzzy_session_updated.files[0].root, "/tmp/remote-project");
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/gateway.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(32),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "remote-fuzzy-session".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStop should succeed through remote gateway");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "remote-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted notification should arrive");
    assert_eq!(fuzzy_session_completed.session_id, "remote-fuzzy-session");

    let windows_setup: WindowsSandboxSetupStartResponse = client
        .request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(40),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/remote-project")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        })
        .await
        .expect("windowsSandbox/setupStart should succeed through remote gateway");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted notification should arrive");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);
}
