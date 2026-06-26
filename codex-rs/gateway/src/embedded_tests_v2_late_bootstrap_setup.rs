use super::*;
use pretty_assertions::assert_eq;

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
                thread_id: None,
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
