use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_health_late_notifications.rs"]
mod embedded_tests_health_late_notifications;

#[path = "embedded_tests_health_late_bootstrap.rs"]
mod embedded_tests_health_late_bootstrap;
#[path = "embedded_tests_health_late_mutation.rs"]
mod embedded_tests_health_late_mutation;
#[path = "embedded_tests_health_late_setup.rs"]
mod embedded_tests_health_late_setup;

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

#[tokio::test]
async fn remote_multi_worker_aggregates_bootstrap_account_and_models_over_v2() {
    let worker_a = start_mock_remote_multi_connection_bootstrap_server(
        OpenAiAuthRequirement::NotRequired,
        Some(vec![("codex", "Codex", 20), ("shared", "Shared", 5)]),
        vec![
            ("shared-model", "Shared Model", true),
            ("worker-a-model", "Worker A Model", false),
        ],
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_bootstrap_server(
        OpenAiAuthRequirement::Required,
        Some(vec![("codex", "Codex", 20), ("worker-b", "Worker B", 35)]),
        vec![
            ("shared-model", "Shared Model", true),
            ("worker-b-model", "Worker B Model", false),
        ],
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

    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time")
    .expect("account/read should aggregate through multi-worker gateway");
    assert_eq!(account.account, None);
    assert_eq!(account.requires_openai_auth, true);

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(2),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should aggregate through multi-worker gateway");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .expect("aggregated rate limits should include per-limit map")
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        HashSet::from(["codex", "shared", "worker-b"])
    );

    let models: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(3),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list should finish in time")
    .expect("model/list should aggregate through multi-worker gateway");
    assert_eq!(models.next_cursor, None);
    assert_eq!(
        models
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model", "worker-a-model", "worker-b-model"]
    );
    assert_eq!(
        models
            .data
            .iter()
            .filter(|model| model.is_default)
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model"]
    );

    let first_model_page: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(5),
            params: ModelListParams {
                cursor: None,
                limit: Some(2),
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list first page should finish in time")
    .expect("model/list first page should aggregate through multi-worker gateway");
    assert_eq!(
        first_model_page.next_cursor.as_deref(),
        Some("model-offset:2")
    );
    assert_eq!(
        first_model_page
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model", "worker-a-model"]
    );

    let second_model_page: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(6),
            params: ModelListParams {
                cursor: first_model_page.next_cursor,
                limit: Some(2),
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list second page should finish in time")
    .expect("model/list second page should aggregate through multi-worker gateway");
    assert_eq!(second_model_page.next_cursor, None);
    assert_eq!(
        second_model_page
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-b-model"]
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
async fn remote_multi_worker_supports_legacy_client_requests_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_server("worker-a").await;
    let worker_b = start_mock_remote_multi_connection_legacy_server("worker-b").await;
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
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .expect("worker B thread should expose rollout path");

    let summary: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(3),
            params: GetConversationSummaryParams::RolloutPath {
                rollout_path: worker_b_rollout_path.clone(),
            },
        }),
    )
    .await
    .expect("getConversationSummary should finish in time")
    .expect("getConversationSummary should route through multi-worker gateway");
    assert_eq!(
        summary.summary.conversation_id.to_string(),
        "00000000-0000-0000-0000-0000000000b2"
    );
    assert_eq!(summary.summary.path, worker_b_rollout_path);
    assert_eq!(summary.summary.preview, "Worker B summary");

    let summary_by_thread_id: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(4),
            params: GetConversationSummaryParams::ThreadId {
                conversation_id: ThreadId::from_string(&worker_b_thread.thread.id)
                    .expect("worker B thread id should parse"),
            },
        }),
    )
    .await
    .expect("getConversationSummary by thread id should finish in time")
    .expect("getConversationSummary by thread id should route to the owning worker");
    assert_eq!(
        summary_by_thread_id.summary.conversation_id,
        ThreadId::from_string(&worker_b_thread.thread.id).expect("worker B thread id should parse")
    );
    assert_eq!(summary_by_thread_id.summary.path, worker_b_rollout_path);
    assert_eq!(summary_by_thread_id.summary.preview, "Worker B summary");

    let diff: GitDiffToRemoteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GitDiffToRemote {
            request_id: RequestId::Integer(5),
            params: GitDiffToRemoteParams {
                cwd: PathBuf::from("/tmp/worker-b/repo"),
            },
        }),
    )
    .await
    .expect("gitDiffToRemote should finish in time")
    .expect("gitDiffToRemote should route through multi-worker gateway");
    assert_eq!(diff.sha.0, "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(diff.diff, "diff --git a/README.md b/README.md\n");

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
async fn remote_multi_worker_restores_thread_read_after_account_exhaustion_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Available,
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

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("worker-b"))
            .and_then(|snapshot| snapshot.rate_limit_reached_type),
        Some(codex_app_server_protocol::RateLimitReachedType::RateLimitReached)
    );

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/read\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let restored: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(4),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read should restore through the replacement worker");
    assert_eq!(restored.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/read handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/read\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

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
async fn remote_multi_worker_restores_thread_rollback_after_account_exhaustion_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Available,
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/rollback\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let restored: ThreadRollbackResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(4),
            params: ThreadRollbackParams {
                thread_id: worker_b_thread.thread.id.clone(),
                num_turns: 1,
            },
        }),
    )
    .await
    .expect("thread/rollback should finish in time")
    .expect("thread/rollback should restore through the replacement worker");
    assert_eq!(restored.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-rollback"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/rollback handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/rollback\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

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
async fn remote_multi_worker_restores_thread_archive_after_account_exhaustion_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Available,
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/archive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let archive_response: ThreadArchiveResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(4),
            params: ThreadArchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/archive should finish in time")
    .expect("thread/archive should restore through the replacement worker");
    assert_eq!(archive_response, ThreadArchiveResponse {});

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/archive handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/archive\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after archive should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
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
async fn remote_multi_worker_restores_thread_unarchive_after_account_exhaustion_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Available,
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/unarchive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let unarchive_response: ThreadUnarchiveResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(4),
            params: ThreadUnarchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/unarchive should finish in time")
    .expect("thread/unarchive should restore through the replacement worker");
    assert_eq!(unarchive_response.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        unarchive_response.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-unarchive"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/unarchive handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/unarchive\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after unarchive should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
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
async fn remote_multi_worker_restores_thread_metadata_update_after_account_exhaustion_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Available,
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/metadata/update\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let metadata_update_response: ThreadMetadataUpdateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(4),
            params: ThreadMetadataUpdateParams {
                thread_id: worker_b_thread.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        }),
    )
    .await
    .expect("thread/metadata/update should finish in time")
    .expect("thread/metadata/update should restore through the replacement worker");
    assert_eq!(
        metadata_update_response.thread.id,
        worker_b_thread.thread.id
    );
    assert_eq!(
        metadata_update_response
            .thread
            .cwd
            .as_ref()
            .to_string_lossy(),
        "/tmp/worker-a-metadata"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/metadata/update handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/metadata/update\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after metadata update should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
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
