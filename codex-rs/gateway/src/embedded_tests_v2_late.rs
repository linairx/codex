use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_v2_late_aggregation.rs"]
mod embedded_tests_v2_late_aggregation;


#[tokio::test]
async fn remote_multi_worker_supports_drop_in_v2_client_bootstrap_setup_workflow() {
    let worker_a = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-a",
            requires_openai_auth: false,
            rate_limits: vec![("codex", "Codex", 20), ("shared", "Shared", 5)],
            models: vec![
                ("shared-model", "Shared Model", true),
                ("worker-a-model", "Worker A Model", false),
            ],
            apps: vec![
                ("shared-app", "Shared App"),
                ("worker-a-app", "Worker A App"),
            ],
            mcp_status_names: vec!["shared-mcp", "worker-a-mcp"],
            shared_cwd: "/tmp/shared-repo",
            unique_cwd: "/tmp/worker-a-only",
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-b",
            requires_openai_auth: true,
            rate_limits: vec![("codex", "Codex", 20), ("worker-b", "Worker B", 35)],
            models: vec![
                ("shared-model", "Shared Model", true),
                ("worker-b-model", "Worker B Model", false),
            ],
            apps: vec![
                ("shared-app", "Shared App"),
                ("worker-b-app", "Worker B App"),
            ],
            mcp_status_names: vec!["shared-mcp", "worker-b-mcp"],
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

    let detected: ExternalAgentConfigDetectResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(4),
            params: ExternalAgentConfigDetectParams {
                include_home: true,
                cwds: Some(vec!["/tmp/shared-repo".into()]),
            },
        }),
    )
    .await
    .expect("externalAgentConfig/detect should finish in time")
    .expect("externalAgentConfig/detect should aggregate through multi-worker gateway");
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

    let apps: AppsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::AppsList {
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
    .expect("app/list should finish in time")
    .expect("app/list should aggregate through multi-worker gateway");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(
        apps.data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-app", "worker-a-app", "worker-b-app"]
    );

    let skills: SkillsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(6),
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

    let mcp_statuses: ListMcpServerStatusResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(7),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: Some(10),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServerStatus/list should finish in time")
    .expect("mcpServerStatus/list should aggregate through multi-worker gateway");
    assert_eq!(mcp_statuses.next_cursor, None);
    assert_eq!(
        mcp_statuses
            .data
            .iter()
            .map(|status| status.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-mcp", "worker-a-mcp", "worker-b-mcp"]
    );

    let mcp_oauth_login: McpServerOauthLoginResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(8),
            params: McpServerOauthLoginParams {
                name: "worker-b-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
            },
        }),
    )
    .await
    .expect("mcpServer/oauth/login should finish in time")
    .expect("mcpServer/oauth/login should succeed through multi-worker gateway");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/worker-b-mcp"
    );

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
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
    .expect("mcpServer/oauthLogin/completed notification should arrive");
    assert_eq!(mcp_oauth_completed.name, "worker-b-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

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

#[tokio::test]
async fn remote_multi_worker_supports_primary_worker_onboarding_and_feedback_flows_over_v2() {
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

    let canceled_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptDeviceCode,
        }),
    )
    .await
    .expect("account/login/start should finish in time")
    .expect("account/login/start should succeed through multi-worker gateway");
    let canceled_login_id = match canceled_login {
        LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            verification_url,
            user_code,
        } => {
            assert_eq!(login_id, "worker-a-login-1");
            assert_eq!(verification_url, "https://example.com/device");
            assert_eq!(user_code, "worker-a-CODE");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let cancel_login: CancelLoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(2),
            params: CancelLoginAccountParams {
                login_id: canceled_login_id,
            },
        }),
    )
    .await
    .expect("account/login/cancel should finish in time")
    .expect("account/login/cancel should succeed through multi-worker gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::Canceled);

    let completed_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(3),
            params: LoginAccountParams::Chatgpt {
                codex_streamlined_login: false,
            },
        }),
    )
    .await
    .expect("second account/login/start should finish in time")
    .expect("second account/login/start should succeed through multi-worker gateway");
    let completed_login_id = match completed_login {
        LoginAccountResponse::Chatgpt { login_id, auth_url } => {
            assert_eq!(login_id, "worker-a-login-2");
            assert_eq!(auth_url, "https://example.com/login");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let login_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/login/completed notification should arrive");
    assert_eq!(login_completed.login_id, Some(completed_login_id));
    assert_eq!(login_completed.success, true);
    assert_eq!(login_completed.error, None);

    let feedback: FeedbackUploadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(4),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway multi-worker parity regression".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        }),
    )
    .await
    .expect("feedback/upload should finish in time")
    .expect("feedback/upload should succeed through multi-worker gateway");
    assert_eq!(feedback.thread_id, "feedback-thread-worker-a");

    let add_credits: SendAddCreditsNudgeEmailResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(5),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        }),
    )
    .await
    .expect("account/sendAddCreditsNudgeEmail should finish in time")
    .expect("account/sendAddCreditsNudgeEmail should succeed through multi-worker gateway");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(6),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf worker-a-command".to_string(),
                ],
                process_id: Some("proc-worker-a".to_string()),
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
    .expect("command/exec should finish in time")
    .expect("command/exec should succeed through multi-worker gateway");
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
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta notification should arrive");
    assert_eq!(command_output.process_id, "proc-worker-a");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "d29ya2VyLWEtY29tbWFuZA==");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(7),
            params: CommandExecWriteParams {
                process_id: "proc-worker-a".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        }),
    )
    .await
    .expect("command/exec/write should finish in time")
    .expect("command/exec/write should succeed through multi-worker gateway");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(8),
            params: CommandExecResizeParams {
                process_id: "proc-worker-a".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        }),
    )
    .await
    .expect("command/exec/resize should finish in time")
    .expect("command/exec/resize should succeed through multi-worker gateway");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(9),
            params: CommandExecTerminateParams {
                process_id: "proc-worker-a".to_string(),
            },
        }),
    )
    .await
    .expect("command/exec/terminate should finish in time")
    .expect("command/exec/terminate should succeed through multi-worker gateway");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

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
async fn remote_multi_worker_supports_fanout_external_auth_onboarding_over_v2() {
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

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("worker-a@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-worker-a"),
    )
    .expect("access token should encode");

    let login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptAuthTokens {
                access_token,
                chatgpt_account_id: "org-worker-a".to_string(),
                chatgpt_plan_type: Some("pro".to_string()),
            },
        }),
    )
    .await
    .expect("account/login/start should finish in time")
    .expect("account/login/start should succeed through multi-worker gateway");
    assert_eq!(login, LoginAccountResponse::ChatgptAuthTokens {});

    let login_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/login/completed notification should arrive");
    assert_eq!(login_completed.login_id, None);
    assert_eq!(login_completed.success, true);
    assert_eq!(login_completed.error, None);

    let account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated notification should arrive");
    assert_eq!(account_updated.auth_mode, Some(AuthMode::ChatgptAuthTokens));
    assert_eq!(account_updated.plan_type, Some(AccountPlanType::Pro));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate external-auth notifications should be suppressed after token login fanout"
    );

    let cancel_login: CancelLoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(2),
            params: CancelLoginAccountParams {
                login_id: "00000000-0000-0000-0000-000000000001".to_string(),
            },
        }),
    )
    .await
    .expect("account/login/cancel should finish in time")
    .expect("account/login/cancel should succeed through multi-worker gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::NotFound);

    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(3),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time")
    .expect("account/read should succeed through multi-worker gateway after login");
    assert_eq!(
        account.account,
        Some(codex_app_server_protocol::Account::Chatgpt {
            email: "worker-a@example.com".to_string(),
            plan_type: AccountPlanType::Pro,
        })
    );
    assert_eq!(account.requires_openai_auth, false);

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
async fn remote_multi_worker_supports_drop_in_v2_client_account_rate_limits_read() {
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

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(1),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should succeed through multi-worker gateway");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(rate_limits.rate_limits.limit_name.as_deref(), Some("Codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .map(HashMap::len),
        Some(2)
    );
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("codex"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Codex")
    );
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("worker-b"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Worker B")
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
async fn remote_multi_worker_fanouts_api_key_login_start_over_v2() {
    let worker_a = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-a",
            requires_openai_auth: true,
            rate_limits: vec![("worker-a", "Worker A", 20)],
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

    let account_before: GetAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("initial account/read should finish in time")
    .expect("initial account/read should succeed through multi-worker gateway");
    assert_eq!(account_before.account, None);
    assert_eq!(account_before.requires_openai_auth, true);

    let login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(2),
            params: LoginAccountParams::ApiKey {
                api_key: "sk-gateway-test".to_string(),
            },
        }),
    )
    .await
    .expect("account/login/start should finish in time")
    .expect("account/login/start should succeed through multi-worker gateway");
    assert_eq!(login, LoginAccountResponse::ApiKey {});

    let account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after api-key login");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
                && notification.auth_mode == Some(AuthMode::ApiKey)
                && notification.plan_type.is_none()
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated notification should arrive after api-key login fanout");
    assert_eq!(account_updated.auth_mode, Some(AuthMode::ApiKey));
    assert_eq!(account_updated.plan_type, None);
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate account/updated should be suppressed after api-key login fanout"
    );

    let account_after: GetAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(3),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("post-login account/read should finish in time")
    .expect("post-login account/read should succeed through multi-worker gateway");
    assert_eq!(
        account_after.account,
        Some(codex_app_server_protocol::Account::ApiKey {})
    );
    assert_eq!(account_after.requires_openai_auth, false);

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
