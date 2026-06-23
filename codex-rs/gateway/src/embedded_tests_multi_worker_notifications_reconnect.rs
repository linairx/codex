use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_forwards_connection_state_notifications_after_worker_reconnect() {
    let websocket_url =
        start_reconnecting_v2_single_worker_connection_state_notification_server().await;
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
    let events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
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
        channel_capacity: 64,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

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
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after worker reconnect");
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
                other => panic!("unexpected notification after worker reconnect: {other:?}"),
            }
        }
    })
    .await
    .expect("connection-state notifications should arrive after reconnect");

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_deduplicates_connection_state_notifications_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_state_notification_server().await;
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

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
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
        channel_capacity: 16,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

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
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after worker reconnect");
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
                other => panic!("unexpected notification after worker reconnect: {other:?}"),
            }
        }
    })
    .await
    .expect("deduplicated state notifications should arrive after reconnect");
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
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate connection-state notifications should still be suppressed after reconnect"
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
