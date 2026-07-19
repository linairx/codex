use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_routes_filesystem_operations_to_primary_worker_over_v2() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_multi_connection_filesystem_operations_server("worker-a").await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_multi_connection_filesystem_operations_server("worker-b").await;
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

    let create_directory: FsCreateDirectoryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(1),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        }),
    )
    .await
    .expect("fs/createDirectory should finish in time")
    .expect("fs/createDirectory should route through multi-worker gateway");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(2),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cHJpbWFyeS1maWxl".to_string(),
            },
        }),
    )
    .await
    .expect("fs/writeFile should finish in time")
    .expect("fs/writeFile should route through multi-worker gateway");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(3),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readFile should finish in time")
    .expect("fs/readFile should route through multi-worker gateway");
    assert_eq!(read_file.data_base64, "d29ya2VyLWEtZmlsZQ==");

    let metadata: FsGetMetadataResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(4),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/getMetadata should finish in time")
    .expect("fs/getMetadata should route through multi-worker gateway");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(5),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readDirectory should finish in time")
    .expect("fs/readDirectory should route through multi-worker gateway");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "worker-a-gateway.txt");

    let copy: FsCopyResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(6),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/worker-a-primary/nested/copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        }),
    )
    .await
    .expect("fs/copy should finish in time")
    .expect("fs/copy should route through multi-worker gateway");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(7),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(false),
                force: Some(true),
            },
        }),
    )
    .await
    .expect("fs/remove should finish in time")
    .expect("fs/remove should route through multi-worker gateway");
    assert_eq!(remove, FsRemoveResponse {});

    let fuzzy_search: FuzzyFileSearchResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(8),
            params: FuzzyFileSearchParams {
                query: "gate".to_string(),
                roots: vec!["/tmp/worker-a-primary".to_string()],
                cancellation_token: Some("multi-worker-fuzzy-search".to_string()),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch should finish in time")
    .expect("fuzzyFileSearch should route through multi-worker gateway");
    assert_eq!(
        fuzzy_search.files,
        vec![
            FuzzyFileSearchResult {
                root: "/tmp/worker-b-search".to_string(),
                path: "src/gateway.rs".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.rs".to_string(),
                score: 60,
                indices: Some(vec![4, 5, 6, 7]),
            },
            FuzzyFileSearchResult {
                root: "/tmp/worker-a-primary".to_string(),
                path: "docs/gateway.md".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.md".to_string(),
                score: 42,
                indices: Some(vec![5, 6, 7, 8]),
            },
        ]
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(9),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "multi-worker-fuzzy-session".to_string(),
                roots: vec!["/tmp/worker-a-primary".to_string()],
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStart should finish in time")
    .expect("fuzzyFileSearch/sessionStart should route through multi-worker gateway");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(10),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "multi-worker-fuzzy-session".to_string(),
                query: "gate".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionUpdate should finish in time")
    .expect("fuzzyFileSearch/sessionUpdate should route through multi-worker gateway");
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
                && notification.session_id == "multi-worker-fuzzy-session"
                && notification.query == "gate"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated notification should arrive");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(fuzzy_session_updated.files[0].root, "/tmp/worker-a-primary");
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/gateway.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(11),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "multi-worker-fuzzy-session".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStop should finish in time")
    .expect("fuzzyFileSearch/sessionStop should route through multi-worker gateway");
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
                && notification.session_id == "multi-worker-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted notification should arrive");
    assert_eq!(
        fuzzy_session_completed.session_id,
        "multi-worker-fuzzy-session"
    );

    let windows_setup: WindowsSandboxSetupStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(12),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/worker-a-primary")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        }),
    )
    .await
    .expect("windowsSandbox/setupStart should finish in time")
    .expect("windowsSandbox/setupStart should route through multi-worker gateway");
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

    assert_eq!(
        *worker_a_requests.lock().await,
        vec![
            "fs/createDirectory".to_string(),
            "fs/writeFile".to_string(),
            "fs/readFile".to_string(),
            "fs/getMetadata".to_string(),
            "fs/readDirectory".to_string(),
            "fs/copy".to_string(),
            "fs/remove".to_string(),
            "fuzzyFileSearch".to_string(),
            "fuzzyFileSearch/sessionStart".to_string(),
            "fuzzyFileSearch/sessionUpdate".to_string(),
            "fuzzyFileSearch/sessionStop".to_string(),
            "windowsSandbox/setupStart".to_string(),
        ]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["fuzzyFileSearch".to_string()]
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
async fn remote_multi_worker_aggregates_setup_discovery_requests_over_v2() {
    let worker_a = start_mock_remote_multi_connection_discovery_server(
        "worker-a",
        "/tmp/shared-repo",
        "/tmp/worker-a-only",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_discovery_server(
        "worker-b",
        "/tmp/shared-repo",
        "/tmp/worker-b-only",
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

    let detected: ExternalAgentConfigDetectResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigDetectParams {
                include_home: true,
                cwds: Some(vec!["/tmp/shared-repo".into()]),
                source: None,
                migration_source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/detect should finish in time")
    .expect("externalAgentConfig/detect should aggregate through multi-worker gateway");
    assert_eq!(detected.items.len(), 3);
    assert_eq!(
        detected
            .items
            .iter()
            .map(|item| item.description.as_str())
            .collect::<Vec<_>>(),
        vec![
            "shared config",
            "worker-a repo config",
            "worker-b repo config",
        ]
    );

    let skills: SkillsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(2),
            params: SkillsListParams {
                cwds: vec!["/tmp/shared-repo".into()],
                force_reload: false,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time")
    .expect("skills/list should aggregate through multi-worker gateway");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(skills.data[0].cwd, PathBuf::from("/tmp/shared-repo"));
    assert_eq!(
        skills.data[0]
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-skill", "worker-a-skill", "worker-b-skill"]
    );
    assert_eq!(
        skills.data[0]
            .errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>(),
        vec!["shared warning", "worker-a warning", "worker-b warning"]
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
