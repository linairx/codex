use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_remote_late_notifications.rs"]
mod embedded_tests_remote_late_notifications;

#[path = "embedded_tests_remote_late_tail.rs"]
mod embedded_tests_remote_late_tail;

#[tokio::test]
async fn remote_single_worker_supports_external_auth_onboarding_flows_over_v2() {
    let websocket_url = start_mock_remote_workflow_server().await;
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
    .expect("remote client should connect to remote gateway");

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("remote@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-remote"),
    )
    .expect("access token should encode");

    let login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptAuthTokens {
                access_token,
                chatgpt_account_id: "org-remote".to_string(),
                chatgpt_plan_type: Some("pro".to_string()),
            },
        })
        .await
        .expect("account/login/start should succeed through remote gateway");
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

    let cancel_login: CancelLoginAccountResponse = client
        .request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(2),
            params: CancelLoginAccountParams {
                login_id: "00000000-0000-0000-0000-000000000001".to_string(),
            },
        })
        .await
        .expect("account/login/cancel should succeed through remote gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::NotFound);

    let account: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(3),
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read should succeed through remote gateway after login");
    assert_eq!(
        account.account,
        Some(codex_app_server_protocol::Account::Chatgpt {
            email: "remote@example.com".to_string(),
            plan_type: AccountPlanType::Pro,
        })
    );
    assert_eq!(account.requires_openai_auth, false);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_forwards_connection_state_notifications_over_v2() {
    let websocket_url = start_mock_remote_multi_connection_state_notification_server().await;
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to remote gateway");

    let mut saw_account_updated = false;
    let mut saw_rate_limits = false;
    let mut saw_app_list = false;
    let mut saw_login_completed = false;
    let mut saw_mcp_oauth_login_completed = false;
    let mut saw_mcp_startup_status = false;
    let mut saw_warning = false;
    let mut saw_config_warning = false;
    let mut saw_deprecation_notice = false;
    let mut saw_windows_world_writable_warning = false;
    let mut saw_windows_sandbox_setup_completed = false;
    timeout(Duration::from_secs(5), async {
        while !(saw_account_updated
            && saw_rate_limits
            && saw_app_list
            && saw_login_completed
            && saw_mcp_oauth_login_completed
            && saw_mcp_startup_status
            && saw_warning
            && saw_config_warning
            && saw_deprecation_notice
            && saw_windows_world_writable_warning
            && saw_windows_sandbox_setup_completed)
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.auth_mode, None);
                    saw_account_updated = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::AccountRateLimitsUpdated(notification),
                ) => {
                    assert_eq!(notification.rate_limits.plan_type, None);
                    saw_rate_limits = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AppListUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.data.len(), 1);
                    assert_eq!(notification.data[0].name, "calendar");
                    saw_app_list = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                    notification,
                )) => {
                    assert_eq!(notification.login_id, None);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_login_completed = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::McpServerOauthLoginCompleted(notification),
                ) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_mcp_oauth_login_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::McpServerStatusUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.status, McpServerStartupState::Ready);
                    assert_eq!(notification.error, None);
                    saw_mcp_startup_status = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::Warning(notification)) => {
                    assert_eq!(notification.thread_id, None);
                    assert_eq!(notification.message, "shared warning");
                    saw_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ConfigWarning(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared config warning");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("check your shared config")
                    );
                    saw_config_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::DeprecationNotice(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared deprecation notice");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("update the shared workflow")
                    );
                    saw_deprecation_notice = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsWorldWritableWarning(notification),
                ) => {
                    assert_eq!(notification.sample_paths, vec!["C:\\shared-temp"]);
                    assert_eq!(notification.extra_count, 2);
                    assert_eq!(notification.failed_scan, false);
                    saw_windows_world_writable_warning = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsSandboxSetupCompleted(notification),
                ) => {
                    assert_eq!(notification.mode, WindowsSandboxSetupMode::Unelevated);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_windows_sandbox_setup_completed = true;
                }
                other => panic!("unexpected notification: {other:?}"),
            }
        }
    })
    .await
    .expect("connection-state notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
