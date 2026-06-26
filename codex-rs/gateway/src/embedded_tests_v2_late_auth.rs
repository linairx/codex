use super::*;
use pretty_assertions::assert_eq;

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
            email: Some("worker-a@example.com".to_string()),
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
