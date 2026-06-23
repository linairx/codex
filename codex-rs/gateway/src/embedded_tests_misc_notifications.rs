use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_deduplicates_skills_changed_notifications_over_v2() {
    let worker_a = start_mock_remote_multi_connection_skills_changed_server().await;
    let worker_b = start_mock_remote_multi_connection_skills_changed_server().await;
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

    let first_notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("initial skills/changed should finish in time")
        .expect("event stream should stay open");
    assert!(matches!(
        first_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate initial skills/changed should be suppressed"
    );

    let _: SkillsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(1),
            params: SkillsListParams {
                cwds: vec!["/tmp/shared-repo".into()],
                force_reload: true,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time")
    .expect("skills/list should succeed through multi-worker gateway");

    let second_notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time")
        .expect("event stream should stay open");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate post-refresh skills/changed should be suppressed"
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
async fn remote_multi_worker_deduplicates_connection_state_notifications_over_v2() {
    let worker_a = start_mock_remote_multi_connection_state_notification_server().await;
    let worker_b = start_mock_remote_multi_connection_state_notification_server().await;
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
        channel_capacity: 16,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let mut saw_account_updated = false;
    let mut saw_rate_limits = false;
    let mut saw_app_list = false;
    let mut saw_login_completed = false;
    let mut saw_mcp_oauth_login_completed = false;
    let mut saw_mcp_startup = false;
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
            && saw_mcp_startup
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
                    saw_mcp_startup = true;
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
    .expect("deduplicated state notifications should arrive");
    assert!(saw_account_updated);
    assert!(saw_rate_limits);
    assert!(saw_app_list);
    assert!(saw_login_completed);
    assert!(saw_mcp_oauth_login_completed);
    assert!(saw_mcp_startup);
    assert!(saw_warning);
    assert!(saw_config_warning);
    assert!(saw_deprecation_notice);
    assert!(saw_windows_world_writable_warning);
    assert!(saw_windows_sandbox_setup_completed);
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate connection-state notifications should be suppressed"
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
