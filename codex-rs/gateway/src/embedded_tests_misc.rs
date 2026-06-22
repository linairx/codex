use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_misc_reconnect.rs"]
mod embedded_tests_misc_reconnect;

#[path = "embedded_tests_misc_healthz.rs"]
mod embedded_tests_misc_healthz;

#[path = "embedded_tests_misc_health.rs"]
mod embedded_tests_misc_health;

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

#[tokio::test]
async fn remote_multi_worker_supports_plugin_discovery_and_management_over_v2() {
    let worker_a = start_mock_remote_multi_plugin_server(
        "worker-a-marketplace",
        "/tmp/worker-a",
        "worker-a-plugin",
        "Worker A plugin detail",
    )
    .await;
    let worker_b = start_mock_remote_multi_plugin_server(
        "worker-b-marketplace",
        "/tmp/worker-b",
        "worker-b-plugin",
        "Worker B plugin detail",
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

    let listed: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(1),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/shared-repo"],
            }))
            .expect("plugin/list params should deserialize"),
        })
        .await
        .expect("plugin/list should aggregate through multi-worker gateway");
    assert_eq!(listed.marketplaces.len(), 2);
    assert_eq!(
        listed
            .marketplaces
            .iter()
            .map(|marketplace| marketplace.name.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-a-marketplace", "worker-b-marketplace"]
    );
    assert_eq!(
        listed.marketplaces[0].plugins[0].installed, false,
        "worker A plugin should start uninstalled"
    );
    assert_eq!(
        listed.marketplaces[1].plugins[0].installed, false,
        "worker B plugin should start uninstalled"
    );

    let plugin: PluginReadResponse = client
        .request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(2),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/worker-b/marketplace.json",
                "pluginName": "worker-b-plugin",
            }))
            .expect("plugin/read params should deserialize"),
        })
        .await
        .expect("plugin/read should route to the matching worker");
    assert_eq!(plugin.plugin.marketplace_name, "worker-b-marketplace");
    assert_eq!(plugin.plugin.summary.name, "worker-b-plugin");
    assert_eq!(
        plugin.plugin.description.as_deref(),
        Some("Worker B plugin detail")
    );

    let install: PluginInstallResponse = client
        .request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(3),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/worker-b/marketplace.json",
                "pluginName": "worker-b-plugin",
            }))
            .expect("plugin/install params should deserialize"),
        })
        .await
        .expect("plugin/install should route to the matching worker");
    assert_eq!(install.auth_policy, PluginAuthPolicy::OnInstall);
    assert!(install.apps_needing_auth.is_empty());

    let listed_after_install: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(4),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/shared-repo"],
            }))
            .expect("plugin/list params after install should deserialize"),
        })
        .await
        .expect("plugin/list after install should aggregate through multi-worker gateway");
    assert_eq!(listed_after_install.marketplaces.len(), 2);
    assert_eq!(
        listed_after_install.marketplaces[0].plugins[0].installed,
        false
    );
    assert_eq!(
        listed_after_install.marketplaces[1].plugins[0].installed,
        true
    );

    let uninstall: PluginUninstallResponse = client
        .request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(5),
            params: PluginUninstallParams {
                plugin_id: "worker-b-plugin@worker-b-marketplace".to_string(),
            },
        })
        .await
        .expect("plugin/uninstall should route to the installed worker");
    assert_eq!(uninstall, PluginUninstallResponse {});

    let listed_after_uninstall: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(6),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/shared-repo"],
            }))
            .expect("plugin/list params after uninstall should deserialize"),
        })
        .await
        .expect("plugin/list after uninstall should aggregate through multi-worker gateway");
    assert_eq!(listed_after_uninstall.marketplaces.len(), 2);
    assert_eq!(
        listed_after_uninstall.marketplaces[0].plugins[0].installed,
        false
    );
    assert_eq!(
        listed_after_uninstall.marketplaces[1].plugins[0].installed,
        false
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
async fn remote_multi_worker_supports_v2_turn_routing_and_notification_fan_in() {
    let worker_a = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "turn-thread-worker-a",
        "hello from worker a",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "turn-thread-worker-b",
        "hello from worker b",
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");
    let mut client = client;

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello worker a".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("first turn/start should route to worker A");
    assert_eq!(first_turn_started.turn.id, "turn-thread-worker-a");
    assert_eq!(first_turn_started.turn.status, TurnStatus::InProgress);

    let second_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello worker b".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("second turn/start should route to worker B");
    assert_eq!(second_turn_started.turn.id, "turn-thread-worker-b");
    assert_eq!(second_turn_started.turn.status, TurnStatus::InProgress);

    let mut lifecycle_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            TurnStreamingCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            TurnStreamingCoverage::default(),
        ),
    ]);
    let expected_turn_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            ("turn-thread-worker-a", "hello from worker a"),
        ),
        (
            second_started.thread.id.clone(),
            ("turn-thread-worker-b", "hello from worker b"),
        ),
    ]);
    let mut extended_notifications_by_thread: HashMap<String, HashSet<&'static str>> =
        HashMap::from([
            (first_started.thread.id.clone(), HashSet::new()),
            (second_started.thread.id.clone(), HashSet::new()),
        ]);

    let turn_fan_in_result = timeout(Duration::from_secs(5), async {
        while lifecycle_by_thread
            .values()
            .any(|coverage| *coverage != expected_turn_streaming_coverage())
            || extended_notifications_by_thread
                .values()
                .any(|notifications| *notifications != expected_extended_turn_notifications())
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => {
                    if let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && matches!(
                            notification.status,
                            ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                        )
                    {
                        coverage.saw_thread_active = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                    {
                        coverage.saw_turn_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id.as_deref() == Some(*expected_turn_id)
                        && notification.run.id == format!("hook-{}", notification.thread_id)
                    {
                        coverage.saw_hook_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && matches!(
                            &notification.item,
                            ThreadItem::AgentMessage {
                                id,
                                text,
                                ..
                            } if id == &format!("msg-{}", notification.thread_id)
                                && text == "streaming answer in progress"
                        )
                    {
                        coverage.saw_item_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, expected_delta)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_agent_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification)) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("plan {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("plan_delta");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("summary {}", notification.thread_id)
                        && notification.summary_index == 0
                    {
                        coverage.saw_reasoning_summary_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.summary_index == 0
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("reasoning_summary_part_added");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("reasoning {}", notification.thread_id)
                        && notification.content_index == 0
                    {
                        coverage.saw_reasoning_text_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TerminalInteraction(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.process_id == format!("proc-{}", notification.thread_id)
                        && notification.stdin == "y\n"
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("terminal_interaction");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("stdout {}", notification.thread_id)
                    {
                        coverage.saw_command_output_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("patch {}", notification.thread_id)
                    {
                        coverage.saw_file_change_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnDiffUpdated(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.diff == format!("diff {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("turn_diff_updated");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnPlanUpdated(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.explanation.as_deref() == Some("gateway multi-worker plan")
                        && notification.plan.len() == 1
                        && notification.plan[0].step == format!("plan {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("turn_plan_updated");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.token_usage.total.total_tokens == 42
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("thread_token_usage_updated");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::McpToolCallProgress(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.message
                            == format!("mcp progress {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("mcp_tool_call_progress");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ContextCompacted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("context_compacted");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ModelRerouted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.from_model == "gpt-5"
                        && notification.to_model == "gpt-5-codex"
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("model_rerouted");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::RawResponseItemCompleted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("raw_response_item_completed");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::Error(notification)) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && !notification.will_retry
                        && notification.error.message
                            == format!("recoverable warning {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("error");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewStarted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.review_id == format!("guardian-{}", notification.thread_id)
                        && notification.target_item_id.as_deref()
                            == Some(&format!("cmd-{}", notification.thread_id))
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("guardian_review_started");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewCompleted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.review_id == format!("guardian-{}", notification.thread_id)
                        && notification.target_item_id.as_deref()
                            == Some(&format!("cmd-{}", notification.thread_id))
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("guardian_review_completed");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id.as_deref() == Some(*expected_turn_id)
                        && notification.run.id == format!("hook-{}", notification.thread_id)
                    {
                        coverage.saw_hook_completed = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && matches!(
                            &notification.item,
                            ThreadItem::AgentMessage {
                                id,
                                text,
                                ..
                            } if id == &format!("msg-{}", notification.thread_id)
                                && text == "streaming answer completed"
                        )
                    {
                        coverage.saw_item_completed = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                        && notification.turn.status == TurnStatus::Completed
                    {
                        coverage.saw_turn_completed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        turn_fan_in_result.is_ok(),
        true,
        "turn notifications should fan in from both workers: lifecycle={lifecycle_by_thread:?} extended={extended_notifications_by_thread:?}"
    );

    assert_eq!(
        lifecycle_by_thread.get(&first_started.thread.id),
        Some(&expected_turn_streaming_coverage())
    );
    assert_eq!(
        lifecycle_by_thread.get(&second_started.thread.id),
        Some(&expected_turn_streaming_coverage())
    );
    assert_eq!(
        extended_notifications_by_thread.get(&first_started.thread.id),
        Some(&expected_extended_turn_notifications())
    );
    assert_eq!(
        extended_notifications_by_thread.get(&second_started.thread.id),
        Some(&expected_extended_turn_notifications())
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_turn_control_routing_and_notification_fan_in() {
    let worker_a = start_mock_remote_multi_connection_turn_control_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "worker a steer delta",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_turn_control_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "worker b steer delta",
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start worker a".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("first turn/start should route to worker A");
    assert_eq!(first_turn_started.turn.id, "turn-thread-worker-a");

    let second_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start worker b".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("second turn/start should route to worker B");
    assert_eq!(second_turn_started.turn.id, "turn-thread-worker-b");

    let first_steer: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(5),
            params: TurnSteerParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer worker a".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: first_turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("first turn/steer should route to worker A");
    assert_eq!(first_steer.turn_id, first_turn_started.turn.id);

    let second_steer: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(6),
            params: TurnSteerParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer worker b".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: second_turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("second turn/steer should route to worker B");
    assert_eq!(second_steer.turn_id, second_turn_started.turn.id);

    let first_interrupt: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(7),
            params: TurnInterruptParams {
                thread_id: first_started.thread.id.clone(),
                turn_id: first_turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("first turn/interrupt should route to worker A");
    assert_eq!(first_interrupt, TurnInterruptResponse {});

    let second_interrupt: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(8),
            params: TurnInterruptParams {
                thread_id: second_started.thread.id.clone(),
                turn_id: second_turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("second turn/interrupt should route to worker B");
    assert_eq!(second_interrupt, TurnInterruptResponse {});

    let mut coverage_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            TurnControlCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            TurnControlCoverage::default(),
        ),
    ]);
    let expected_turn_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            ("turn-thread-worker-a", "worker a steer delta"),
        ),
        (
            second_started.thread.id.clone(),
            ("turn-thread-worker-b", "worker b steer delta"),
        ),
    ]);

    let turn_control_result = timeout(Duration::from_secs(5), async {
        while coverage_by_thread.values().any(|coverage| {
            *coverage
                != TurnControlCoverage {
                    saw_thread_active: true,
                    saw_turn_started: true,
                    saw_agent_delta: true,
                    saw_turn_completed: true,
                    saw_thread_idle: true,
                }
        }) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id) {
                        match notification.status {
                            ThreadStatus::Active { ref active_flags }
                                if active_flags.is_empty() =>
                            {
                                coverage.saw_thread_active = true;
                            }
                            ThreadStatus::Idle => {
                                coverage.saw_thread_idle = true;
                            }
                            _ => {}
                        }
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                    {
                        coverage.saw_turn_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, expected_delta)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_agent_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                        && notification.turn.status == TurnStatus::Completed
                    {
                        coverage.saw_turn_completed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        turn_control_result.is_ok(),
        true,
        "turn control notifications should fan in from both workers: {coverage_by_thread:?}"
    );

    assert_eq!(
        coverage_by_thread.get(&first_started.thread.id),
        Some(&TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        })
    );
    assert_eq!(
        coverage_by_thread.get(&second_started.thread.id),
        Some(&TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        })
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_thread_control_and_review_routing() {
    let worker_a = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "turn-worker-a",
        "delta from worker a",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "turn-worker-b",
        "delta from worker b",
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(3),
            params: ThreadUnsubscribeParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/unsubscribe should route to worker A");
    assert_eq!(
        first_unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let second_unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(4),
            params: ThreadUnsubscribeParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/unsubscribe should route to worker B");
    assert_eq!(
        second_unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let first_archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(5),
            params: ThreadArchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/archive should route to worker A");
    assert_eq!(first_archive, ThreadArchiveResponse {});

    let second_archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(6),
            params: ThreadArchiveParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/archive should route to worker B");
    assert_eq!(second_archive, ThreadArchiveResponse {});

    let first_unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(7),
            params: ThreadUnarchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/unarchive should route to worker A");
    assert_eq!(first_unarchive.thread.id, first_started.thread.id);
    assert_eq!(first_unarchive.thread.preview, "/tmp/worker-a");

    let second_unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(8),
            params: ThreadUnarchiveParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/unarchive should route to worker B");
    assert_eq!(second_unarchive.thread.id, second_started.thread.id);
    assert_eq!(second_unarchive.thread.preview, "/tmp/worker-b");

    let expected_thread_lifecycle_notifications = HashSet::from([
        (first_started.thread.id.clone(), "closed"),
        (second_started.thread.id.clone(), "closed"),
        (first_started.thread.id.clone(), "archived"),
        (second_started.thread.id.clone(), "archived"),
        (first_started.thread.id.clone(), "unarchived"),
        (second_started.thread.id.clone(), "unarchived"),
    ]);
    let mut thread_lifecycle_notifications = HashSet::new();
    let lifecycle_result = timeout(Duration::from_secs(5), async {
        while thread_lifecycle_notifications != expected_thread_lifecycle_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "closed"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadArchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "archived"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadUnarchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "unarchived"));
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        lifecycle_result.is_ok(),
        "thread lifecycle notifications should fan in from both workers: {thread_lifecycle_notifications:?}"
    );

    let first_metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(9),
            params: ThreadMetadataUpdateParams {
                thread_id: first_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-worker-a".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("first thread/metadata/update should route to worker A");
    assert_eq!(first_metadata_update.thread.id, first_started.thread.id);

    let second_metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: second_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-worker-b".to_string())),
                    branch: Some(Some("develop".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("second thread/metadata/update should route to worker B");
    assert_eq!(second_metadata_update.thread.id, second_started.thread.id);

    let first_turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(11),
            params: ThreadTurnsListParams {
                thread_id: first_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("first thread/turns/list should route to worker A");
    assert_eq!(first_turns.data.len(), 1);
    assert_eq!(first_turns.data[0].id, "turn-worker-a");
    assert_eq!(first_turns.next_cursor, None);
    assert_eq!(first_turns.backwards_cursor, None);

    let second_turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(12),
            params: ThreadTurnsListParams {
                thread_id: second_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("second thread/turns/list should route to worker B");
    assert_eq!(second_turns.data.len(), 1);
    assert_eq!(second_turns.data[0].id, "turn-worker-b");
    assert_eq!(second_turns.next_cursor, None);
    assert_eq!(second_turns.backwards_cursor, None);

    let first_increment: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(13),
            params: ThreadIncrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/increment_elicitation should route to worker A");
    assert_eq!(
        first_increment,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let second_increment: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(14),
            params: ThreadIncrementElicitationParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/increment_elicitation should route to worker B");
    assert_eq!(
        second_increment,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let first_decrement: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(15),
            params: ThreadDecrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/decrement_elicitation should route to worker A");
    assert_eq!(
        first_decrement,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let second_decrement: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadDecrementElicitationParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/decrement_elicitation should route to worker B");
    assert_eq!(
        second_decrement,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let first_inject: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(17),
            params: ThreadInjectItemsParams {
                thread_id: first_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "worker-a injected"}],
                })],
            },
        })
        .await
        .expect("first thread/inject_items should route to worker A");
    assert_eq!(first_inject, ThreadInjectItemsResponse {});

    let second_inject: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(18),
            params: ThreadInjectItemsParams {
                thread_id: second_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "worker-b injected"}],
                })],
            },
        })
        .await
        .expect("second thread/inject_items should route to worker B");
    assert_eq!(second_inject, ThreadInjectItemsResponse {});

    let first_compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/compact/start should route to worker A");
    assert_eq!(first_compact, ThreadCompactStartResponse {});

    let second_compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(20),
            params: ThreadCompactStartParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/compact/start should route to worker B");
    assert_eq!(second_compact, ThreadCompactStartResponse {});

    let first_shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(21),
            params: ThreadShellCommandParams {
                thread_id: first_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("first thread/shellCommand should route to worker A");
    assert_eq!(first_shell_command, ThreadShellCommandResponse {});

    let second_shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(22),
            params: ThreadShellCommandParams {
                thread_id: second_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("second thread/shellCommand should route to worker B");
    assert_eq!(second_shell_command, ThreadShellCommandResponse {});

    let first_clean: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(23),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/backgroundTerminals/clean should route to worker A");
    assert_eq!(first_clean, ThreadBackgroundTerminalsCleanResponse {});

    let second_clean: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(24),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/backgroundTerminals/clean should route to worker B");
    assert_eq!(second_clean, ThreadBackgroundTerminalsCleanResponse {});

    let first_rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(25),
            params: ThreadRollbackParams {
                thread_id: first_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("first thread/rollback should route to worker A");
    assert_eq!(first_rollback.thread.id, first_started.thread.id);
    assert_eq!(first_rollback.thread.preview, "/tmp/worker-a");

    let second_rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(26),
            params: ThreadRollbackParams {
                thread_id: second_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("second thread/rollback should route to worker B");
    assert_eq!(second_rollback.thread.id, second_started.thread.id);
    assert_eq!(second_rollback.thread.preview, "/tmp/worker-b");

    let first_review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(27),
            params: ReviewStartParams {
                thread_id: first_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review worker A".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("first review/start should route to worker A");
    assert_eq!(first_review.turn.id, "turn-review-thread-worker-a");
    assert_eq!(first_review.review_thread_id, "thread-worker-a-review");

    let second_review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(28),
            params: ReviewStartParams {
                thread_id: second_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review worker B".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("second review/start should route to worker B");
    assert_eq!(second_review.turn.id, "turn-review-thread-worker-b");
    assert_eq!(second_review.review_thread_id, "thread-worker-b-review");

    let first_review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(29),
            params: ThreadReadParams {
                thread_id: first_review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("first review thread/read should route to worker A");
    assert_eq!(first_review_thread.thread.id, first_review.review_thread_id);
    assert_eq!(first_review_thread.thread.preview, "/tmp/worker-a/review");

    let second_review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(30),
            params: ThreadReadParams {
                thread_id: second_review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("second review thread/read should route to worker B");
    assert_eq!(
        second_review_thread.thread.id,
        second_review.review_thread_id
    );
    assert_eq!(second_review_thread.thread.preview, "/tmp/worker-b/review");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_drop_in_v2_client_realtime_list_voices() {
    let worker_a = start_mock_remote_multi_connection_realtime_server_with_voices(
        "thread-worker-a",
        "/tmp/worker-a",
        "session-worker-a",
        "delta from worker a",
        "done from worker a",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Juniper, RealtimeVoice::Maple],
            v2: vec![RealtimeVoice::Alloy],
            default_v1: RealtimeVoice::Juniper,
            default_v2: RealtimeVoice::Alloy,
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_realtime_server_with_voices(
        "thread-worker-b",
        "/tmp/worker-b",
        "session-worker-b",
        "delta from worker b",
        "done from worker b",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Maple, RealtimeVoice::Cove],
            v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
            default_v1: RealtimeVoice::Cove,
            default_v2: RealtimeVoice::Marin,
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let voices: ThreadRealtimeListVoicesResponse = client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed through multi-worker gateway");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList {
                v1: vec![
                    RealtimeVoice::Juniper,
                    RealtimeVoice::Maple,
                    RealtimeVoice::Cove,
                ],
                v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
                default_v1: RealtimeVoice::Juniper,
                default_v2: RealtimeVoice::Alloy,
            },
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_realtime_routing_and_notification_fan_in() {
    let worker_a = start_mock_remote_multi_connection_realtime_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "session-worker-a",
        "delta from worker a",
        "done from worker a",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_realtime_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "session-worker-b",
        "delta from worker b",
        "done from worker b",
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");
    let mut client = client;

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_realtime_started: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: first_started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("first realtime start should route to worker A");
    assert_eq!(first_realtime_started, ThreadRealtimeStartResponse {});

    let second_realtime_started: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeStartParams {
                thread_id: second_started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("second realtime start should route to worker B");
    assert_eq!(second_realtime_started, ThreadRealtimeStartResponse {});

    let first_append: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendTextParams {
                thread_id: first_started.thread.id.clone(),
                text: "hello realtime a".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("first realtime appendText should route to worker A");
    assert_eq!(first_append, ThreadRealtimeAppendTextResponse {});

    let second_append: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeAppendTextParams {
                thread_id: second_started.thread.id.clone(),
                text: "hello realtime b".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("second realtime appendText should route to worker B");
    assert_eq!(second_append, ThreadRealtimeAppendTextResponse {});

    let first_append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(7),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: first_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-a".to_string()),
                },
            },
        })
        .await
        .expect("first realtime appendAudio should route to worker A");
    assert_eq!(first_append_audio, ThreadRealtimeAppendAudioResponse {});

    let second_append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(8),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: second_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "BAUG".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-b".to_string()),
                },
            },
        })
        .await
        .expect("second realtime appendAudio should route to worker B");
    assert_eq!(second_append_audio, ThreadRealtimeAppendAudioResponse {});

    let first_stop: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(9),
            params: ThreadRealtimeStopParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first realtime stop should route to worker A");
    assert_eq!(first_stop, ThreadRealtimeStopResponse {});

    let second_stop: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(10),
            params: ThreadRealtimeStopParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second realtime stop should route to worker B");
    assert_eq!(second_stop, ThreadRealtimeStopResponse {});

    let mut coverage_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
    ]);
    let expected_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            (
                "session-worker-a",
                "delta from worker a",
                "done from worker a",
            ),
        ),
        (
            second_started.thread.id.clone(),
            (
                "session-worker-b",
                "delta from worker b",
                "done from worker b",
            ),
        ),
    ]);

    let realtime_fan_in_result = timeout(Duration::from_secs(5), async {
        while coverage_by_thread.values().any(|coverage| {
            *coverage
                != RealtimeStreamingCoverage {
                    saw_started: true,
                    saw_item_added: true,
                    saw_output_audio_delta: true,
                    saw_transcript_delta: true,
                    saw_transcript_done: true,
                    saw_sdp: true,
                    saw_error: true,
                    saw_closed: true,
                }
        }) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) => {
                    if let Some((expected_session_id, _, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.realtime_session_id.as_deref() == Some(*expected_session_id)
                        && notification.version == RealtimeConversationVersion::V2
                    {
                        coverage.saw_started = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.item["type"] == "message"
                    {
                        coverage.saw_item_added = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.audio.sample_rate == 24_000
                        && notification.audio.num_channels == 1
                        && notification.audio.data
                            == if notification.thread_id == first_started.thread.id {
                                "AQID"
                            } else {
                                "BAUG"
                            }
                    {
                        coverage.saw_output_audio_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) => {
                    if let Some((_, expected_delta, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.delta == *expected_delta
                        && notification.role == "assistant"
                    {
                        coverage.saw_transcript_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) => {
                    if let Some((_, _, expected_text)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.text == *expected_text
                        && notification.role == "assistant"
                    {
                        coverage.saw_transcript_done = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.sdp.contains("s=Codex")
                    {
                        coverage.saw_sdp = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.message == "realtime transport warning"
                    {
                        coverage.saw_error = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.reason.as_deref() == Some("client requested stop")
                    {
                        coverage.saw_closed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        realtime_fan_in_result.is_ok(),
        true,
        "realtime notifications should fan in from both workers: {coverage_by_thread:?}"
    );

    assert_eq!(
        coverage_by_thread.get(&first_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );
    assert_eq!(
        coverage_by_thread.get(&second_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_server_request_routing_and_id_translation() {
    let worker_a = start_mock_remote_multi_connection_server_request_server(
        "018f0000-0000-7000-8000-0000000000a1",
        "/tmp/worker-a",
        "safe-a",
        "acct-worker-a",
        LegacyApprovalExercise::Exercise,
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_server_request_server(
        "018f0000-0000-7000-8000-0000000000b2",
        "/tmp/worker-b",
        "safe-b",
        "acct-worker-b",
        LegacyApprovalExercise::Exercise,
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

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let expected_threads = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let mut gateway_request_ids = HashSet::new();
    let mut user_input_threads = HashSet::new();
    let mut command_threads = HashSet::new();
    let mut file_threads = HashSet::new();
    let mut permissions_threads = HashSet::new();
    let mut mcp_threads = HashSet::new();
    let mut dynamic_tool_call_threads = HashSet::new();
    let mut refresh_accounts = HashSet::new();
    let mut legacy_exec_threads = HashSet::new();
    let mut legacy_patch_threads = HashSet::new();
    timeout(Duration::from_secs(5), async {
        while user_input_threads.len() < expected_threads.len()
            || command_threads.len() < expected_threads.len()
            || file_threads.len() < expected_threads.len()
            || permissions_threads.len() < expected_threads.len()
            || mcp_threads.len() < expected_threads.len()
            || dynamic_tool_call_threads.len() < expected_threads.len()
            || refresh_accounts.len() < expected_threads.len()
            || legacy_exec_threads.len() < expected_threads.len()
            || legacy_patch_threads.len() < expected_threads.len()
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.item_id, "tool-call-remote-workflow");
                    assert_eq!(params.questions.len(), 1);
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-server-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    let answer = if params.thread_id == first_started.thread.id {
                        "safe-a"
                    } else if params.thread_id == second_started.thread.id {
                        "safe-b"
                    } else {
                        panic!("unexpected thread id: {}", params.thread_id);
                    };
                    let mut answers = HashMap::new();
                    answers.insert(
                        "mode".to_string(),
                        ToolRequestUserInputAnswer {
                            answers: vec![answer.to_string()],
                        },
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ToolRequestUserInputResponse { answers })
                                .expect("server request response should serialize"),
                        )
                        .await
                        .expect("server request should resolve");
                    user_input_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "cmd-remote-workflow");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-command-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                                decision: CommandExecutionApprovalDecision::Accept,
                            })
                            .expect("command approval response should serialize"),
                        )
                        .await
                        .expect("command approval should resolve");
                    command_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "file-remote-workflow");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-file-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(FileChangeRequestApprovalResponse {
                                decision: FileChangeApprovalDecision::Accept,
                            })
                            .expect("file approval response should serialize"),
                        )
                        .await
                        .expect("file approval should resolve");
                    file_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "perm-remote-workflow");
                    assert_eq!(params.reason, Some("Need wider permissions".to_string()));
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-permissions-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(PermissionsRequestApprovalResponse {
                                strict_auto_review: None,

                                permissions: codex_app_server_protocol::GrantedPermissionProfile {
                                    network: params.permissions.network,
                                    file_system: params.permissions.file_system,
                                },
                                scope: PermissionGrantScope::Turn,
                            })
                            .expect("permissions approval response should serialize"),
                        )
                        .await
                        .expect("permissions approval should resolve");
                    permissions_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                    assert_eq!(params.server_name, "mock-mcp");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-mcp-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(McpServerElicitationRequestResponse {
                                action: McpServerElicitationAction::Accept,
                                content: Some(serde_json::json!({
                                    "confirmed": true,
                                })),
                                meta: None,
                            })
                            .expect("mcp elicitation response should serialize"),
                        )
                        .await
                        .expect("mcp elicitation should resolve");
                    mcp_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::DynamicToolCall {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.call_id, "tool-call-remote-workflow");
                    assert_eq!(params.tool, "image-edit");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-dynamic-tool-call-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    assert_eq!(
                        params.arguments,
                        serde_json::json!({
                            "prompt": format!("Sharpen image for {}", params.thread_id),
                            "strength": 0.5,
                        })
                    );
                    let thread_id = params.thread_id.clone();
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: format!("tool output for {thread_id}"),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool call response should serialize"),
                        )
                        .await
                        .expect("dynamic tool call should resolve");
                    dynamic_tool_call_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) => {
                    assert_eq!(notification.turn_id, "turn-remote-workflow");
                    assert!(matches!(
                            &notification.item,
                            ThreadItem::DynamicToolCall {
                    namespace: None,
                                id,
                                tool,
                                status,
                                ..
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && *status == DynamicToolCallStatus::InProgress
                        ));
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) => {
                    assert_eq!(notification.turn_id, "turn-remote-workflow");
                    assert!(matches!(
                            &notification.item,
                            ThreadItem::DynamicToolCall {
                    namespace: None,
                                id,
                                tool,
                                status,
                                success,
                                ..
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && *status == DynamicToolCallStatus::Completed
                                && *success == Some(true)
                        ));
                }
                AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                    request_id,
                    params,
                }) => {
                    let previous_account_id = params
                        .previous_account_id
                        .as_deref()
                        .expect("refresh request should include previous account id");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-chatgpt-refresh-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                access_token: format!("access-token-{previous_account_id}"),
                                chatgpt_account_id: previous_account_id.to_string(),
                                chatgpt_plan_type: Some("pro".to_string()),
                            })
                            .expect("chatgpt refresh response should serialize"),
                        )
                        .await
                        .expect("chatgpt refresh should resolve");
                    refresh_accounts.insert(previous_account_id.to_string());
                }
                AppServerEvent::ServerRequest(ServerRequest::ExecCommandApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.call_id, "legacy-exec-call");
                    assert_eq!(params.approval_id, Some("legacy-exec-approval".to_string()));
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-legacy-exec-approval".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ExecCommandApprovalResponse {
                                decision: ReviewDecision::Approved,
                            })
                            .expect("legacy exec approval response should serialize"),
                        )
                        .await
                        .expect("legacy exec approval should resolve");
                    legacy_exec_threads.insert(params.conversation_id.to_string());
                }
                AppServerEvent::ServerRequest(ServerRequest::ApplyPatchApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.call_id, "legacy-patch-call");
                    assert_eq!(
                        params.reason,
                        Some("Need legacy patch approval".to_string())
                    );
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-legacy-patch-approval".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ApplyPatchApprovalResponse {
                                decision: ReviewDecision::ApprovedForSession,
                            })
                            .expect("legacy apply-patch approval response should serialize"),
                        )
                        .await
                        .expect("legacy apply-patch approval should resolve");
                    legacy_patch_threads.insert(params.conversation_id.to_string());
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    _,
                )) => {}
                other => panic!("unexpected event: {other:?}"),
            }
        }
    })
    .await
    .expect("server requests should arrive from both workers");
    assert_eq!(user_input_threads, expected_threads);
    assert_eq!(command_threads, expected_threads);
    assert_eq!(file_threads, expected_threads);
    assert_eq!(permissions_threads, expected_threads);
    assert_eq!(mcp_threads, expected_threads);
    assert_eq!(dynamic_tool_call_threads, expected_threads);
    assert_eq!(
        refresh_accounts,
        HashSet::from(["acct-worker-a".to_string(), "acct-worker-b".to_string(),])
    );
    assert_eq!(legacy_exec_threads, expected_threads);
    assert_eq!(legacy_patch_threads, expected_threads);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_concurrent_overlapping_server_request_ids_over_v2() {
    let request_barrier = Arc::new(Barrier::new(2));
    let worker_a = start_mock_remote_multi_connection_concurrent_server_request_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "safe-a",
        request_barrier.clone(),
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_concurrent_server_request_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "safe-b",
        request_barrier,
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

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let expected_threads = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let health_client = reqwest::Client::new();
    macro_rules! assert_concurrent_pending_server_request_health {
        (
                $stage:literal,
                $backlog_count:expr,
                $answered_count:expr,
                $worker_answered_count:expr,
                $method_counts:expr
            ) => {{
            let mut last_health = None;
            timeout(Duration::from_secs(5), async {
                loop {
                    let response = health_client
                        .get(format!("http://{}/healthz", server.local_addr()))
                        .send()
                        .await
                        .expect("healthz response");
                    let health: GatewayHealthResponse = response.json().await.expect("health body");
                    if health
                        .v2_connections
                        .active_connection_server_request_backlog_method_counts
                        == $method_counts
                    {
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_max_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_peak_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_started_at
                                .is_some(),
                            true
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_answered_but_unresolved_server_request_count,
                            $answered_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_worker_counts,
                            vec![
                                GatewayV2ServerRequestBacklogWorkerCounts {
                                    worker_id: Some(0),
                                    pending_server_request_count: 1,
                                    answered_but_unresolved_server_request_count:
                                        $worker_answered_count,
                                    server_request_backlog_count: 1 + $worker_answered_count,
                                },
                                GatewayV2ServerRequestBacklogWorkerCounts {
                                    worker_id: Some(1),
                                    pending_server_request_count: 1,
                                    answered_but_unresolved_server_request_count:
                                        $worker_answered_count,
                                    server_request_backlog_count: 1 + $worker_answered_count,
                                },
                            ]
                        );
                        break;
                    }
                    last_health = Some(health);
                    sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "concurrent server-request health should settle for {}: {:#?}",
                    $stage, last_health
                )
            });
        }};
    }

    let mut gateway_request_ids = HashSet::new();
    let mut user_input_threads = HashSet::new();
    let mut permissions_threads = HashSet::new();
    let mut mcp_threads = HashSet::new();
    let mut refresh_accounts = HashSet::new();
    let mut user_input_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while user_input_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.item_id, "tool-call-remote-workflow");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-server-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local requests are pending"
                        );
                        user_input_requests.push((request_id, params.thread_id));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent user-input server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "user-input",
        2,
        0,
        0,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "item/tool/requestUserInput".to_string(),
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 2,
        }]
    );
    for (request_id, thread_id) in user_input_requests {
        let answer = match thread_id.as_str() {
            "thread-worker-a" => "safe-a",
            "thread-worker-b" => "safe-b",
            thread_id => panic!("unexpected thread id: {thread_id}"),
        };
        let mut answers = HashMap::new();
        answers.insert(
            "mode".to_string(),
            ToolRequestUserInputAnswer {
                answers: vec![answer.to_string()],
            },
        );
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ToolRequestUserInputResponse { answers })
                    .expect("server request response should serialize"),
            )
            .await
            .expect("server request should resolve");
        user_input_threads.insert(thread_id);
    }

    let mut permission_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while permission_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "perm-remote-workflow");
                        assert_eq!(
                            params.reason,
                            Some("Need concurrent permissions".to_string())
                        );
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-permissions-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local permission requests are pending"
                        );
                        permission_requests.push((request_id, params));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent permission server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "permissions",
        4,
        2,
        1,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, params) in permission_requests {
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(PermissionsRequestApprovalResponse {
                    strict_auto_review: None,

                    permissions: codex_app_server_protocol::GrantedPermissionProfile {
                        network: params.permissions.network,
                        file_system: params.permissions.file_system,
                    },
                    scope: PermissionGrantScope::Turn,
                })
                .expect("permissions approval response should serialize"),
            )
            .await
            .expect("permissions approval should resolve");
        permissions_threads.insert(params.thread_id);
    }

    let mut mcp_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while mcp_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                        assert_eq!(params.server_name, "mock-mcp");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-mcp-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local elicitation requests are pending"
                        );
                        mcp_requests.push((request_id, params));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent mcp elicitation server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "mcp elicitation",
        6,
        4,
        2,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, params) in mcp_requests {
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Accept,
                    content: Some(serde_json::json!({
                        "confirmed": true,
                    })),
                    meta: None,
                })
                .expect("mcp elicitation response should serialize"),
            )
            .await
            .expect("mcp elicitation should resolve");
        mcp_threads.insert(params.thread_id);
    }

    let mut refresh_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while refresh_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                        request_id,
                        params,
                    }) => {
                        let previous_account_id = params
                            .previous_account_id
                            .clone()
                            .expect("refresh request should include previous account id");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-chatgpt-refresh-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local refresh requests are pending"
                        );
                        refresh_requests.push((request_id, previous_account_id));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent ChatGPT token-refresh server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "ChatGPT token refresh",
        8,
        6,
        3,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, previous_account_id) in refresh_requests {
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                    access_token: format!("access-token-{previous_account_id}"),
                    chatgpt_account_id: previous_account_id.clone(),
                    chatgpt_plan_type: Some("pro".to_string()),
                })
                .expect("chatgpt refresh response should serialize"),
            )
            .await
            .expect("chatgpt refresh should resolve");
        refresh_accounts.insert(previous_account_id);
    }
    assert_eq!(user_input_threads, expected_threads);
    assert_eq!(permissions_threads, expected_threads);
    assert_eq!(mcp_threads, expected_threads);
    assert_eq!(
        refresh_accounts,
        HashSet::from([
            "account-thread-worker-a".to_string(),
            "account-thread-worker-b".to_string(),
        ])
    );
    assert_eq!(gateway_request_ids.len(), 8);

    assert_remote_client_shutdown(client.shutdown().await);
    let settled_health = timeout(Duration::from_secs(15), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health
                    .v2_connections
                    .last_connection_server_request_backlog_method_counts
                    .len()
                    == 4
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("last connection server-request backlog health should settle");
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![
            GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(0),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_count: 4,
            },
            GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(1),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_count: 4,
            },
        ]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_routes_thread_mutations_to_owning_workers() {
    let worker_a =
        start_mock_remote_multi_connection_mutation_server("thread-worker-a", "/tmp/worker-a")
            .await;
    let worker_b =
        start_mock_remote_multi_connection_mutation_server("thread-worker-b", "/tmp/worker-b")
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

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        })
        .await
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let expected_started_notifications = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let mut started_notifications = HashSet::new();
    timeout(Duration::from_secs(5), async {
        while started_notifications != expected_started_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadStarted(
                notification,
            )) = event
                && expected_started_notifications.contains(&notification.thread.id)
            {
                started_notifications.insert(notification.thread.id);
            }
        }
    })
    .await
    .expect("thread/started notifications should fan in from both workers");

    let first_name = "Worker A Thread".to_string();
    let second_name = "Worker B Thread".to_string();

    let first_rename: ThreadSetNameResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(3),
            params: ThreadSetNameParams {
                thread_id: first_started.thread.id.clone(),
                name: first_name.clone(),
            },
        }),
    )
    .await
    .expect("first thread/name/set should finish in time")
    .expect("first thread/name/set should route to worker A");
    assert_eq!(first_rename, ThreadSetNameResponse {});

    let second_rename: ThreadSetNameResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(4),
            params: ThreadSetNameParams {
                thread_id: second_started.thread.id.clone(),
                name: second_name.clone(),
            },
        }),
    )
    .await
    .expect("second thread/name/set should finish in time")
    .expect("second thread/name/set should route to worker B");
    assert_eq!(second_rename, ThreadSetNameResponse {});

    let expected_rename_notifications = HashMap::from([
        (first_started.thread.id.clone(), first_name.clone()),
        (second_started.thread.id.clone(), second_name.clone()),
    ]);
    let mut rename_notifications = HashMap::new();
    timeout(Duration::from_secs(5), async {
        while rename_notifications != expected_rename_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && let Some(thread_name) = notification.thread_name
                && expected_rename_notifications
                    .get(&notification.thread_id)
                    .is_some_and(|expected| expected == &thread_name)
            {
                rename_notifications.insert(notification.thread_id, thread_name);
            }
        }
    })
    .await
    .expect("thread/name/updated notifications should fan in from both workers");

    let first_memory_mode: ThreadMemoryModeSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(5),
            params: ThreadMemoryModeSetParams {
                thread_id: first_started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        }),
    )
    .await
    .expect("first thread/memoryMode/set should finish in time")
    .expect("first thread/memoryMode/set should route to worker A");
    assert_eq!(first_memory_mode, ThreadMemoryModeSetResponse {});

    let second_memory_mode: ThreadMemoryModeSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(6),
            params: ThreadMemoryModeSetParams {
                thread_id: second_started.thread.id.clone(),
                mode: ThreadMemoryMode::Disabled,
            },
        }),
    )
    .await
    .expect("second thread/memoryMode/set should finish in time")
    .expect("second thread/memoryMode/set should route to worker B");
    assert_eq!(second_memory_mode, ThreadMemoryModeSetResponse {});

    let first_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(7),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("first thread/read should finish in time")
    .expect("thread/read should route renamed thread back to worker A");
    assert_eq!(first_read.thread.id, first_started.thread.id);
    assert_eq!(first_read.thread.name, Some(first_name.clone()));
    assert_eq!(
        first_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );

    let second_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: second_started.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("second thread/read should finish in time")
    .expect("thread/read should route renamed thread back to worker B");
    assert_eq!(second_read.thread.id, second_started.thread.id);
    assert_eq!(second_read.thread.name, Some(second_name.clone()));
    assert_eq!(
        second_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b"
    );

    let listed: AppServerThreadListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(9),
            params: ThreadListParams {
                parent_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        }),
    )
    .await
    .expect("thread/list should finish in time")
    .expect("thread/list should aggregate renamed threads");
    let names_by_thread = listed
        .data
        .into_iter()
        .map(|thread| (thread.id, thread.name))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        names_by_thread.get(&first_started.thread.id),
        Some(&Some(first_name))
    );
    assert_eq!(
        names_by_thread.get(&second_started.thread.id),
        Some(&Some(second_name))
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_runtime_streams_worker_reconnect_events_over_sse() {
    let worker_a = start_reconnecting_mock_remote_server(Some("secret-token".to_string())).await;
    let worker_b = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-worker-b",
        "/tmp/worker-b",
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
                        websocket_url: worker_a.clone(),

                        auth_token: Some("secret-token".to_string()),
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b.clone(),

                        auth_token: Some("secret-token".to_string()),
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
    let mut events_response = client
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

    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first_thread: ThreadResponse = first_response.json().await.expect("first thread");
    assert_eq!(first_thread.thread.id, "thread-worker-a-1");

    let (reconnecting_event, reconnected_event) = timeout(Duration::from_secs(5), async {
        let mut buffered = String::new();
        let mut reconnecting_event = None;
        let mut reconnected_event = None;
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

                if event.contains("event: gateway/reconnecting") && event.contains(&worker_a) {
                    reconnecting_event = Some(event);
                } else if event.contains("event: gateway/reconnected") && event.contains(&worker_a)
                {
                    reconnected_event = Some(event);
                }

                if let (Some(reconnecting_event), Some(reconnected_event)) =
                    (reconnecting_event.as_ref(), reconnected_event.as_ref())
                {
                    return (reconnecting_event.clone(), reconnected_event.clone());
                }
            }
        }
    })
    .await
    .expect("timed out waiting for worker reconnect events");

    assert_eq!(
        reconnecting_event.contains("event: gateway/reconnecting"),
        true
    );
    assert_eq!(reconnecting_event.contains("\"workerId\":"), true);
    assert_eq!(reconnecting_event.contains(&worker_a), true);
    assert_eq!(
        reconnecting_event.contains("\"reason\":\"remote app server"),
        true
    );

    assert_eq!(
        reconnected_event.contains("event: gateway/reconnected"),
        true
    );
    assert_eq!(reconnected_event.contains("\"workerId\":"), true);
    assert_eq!(reconnected_event.contains(&worker_a), true);

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-b".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("second response");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(second_thread.thread.id, "thread-worker-b");

    let settled_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("expected remote worker health");
            };
            if remote_workers[0].healthy && !remote_workers[0].reconnecting {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("worker health should settle after reconnect");
    let remote_workers = settled_health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers[0].websocket_url, worker_a);
    assert_eq!(remote_workers[0].healthy, true);
    assert_eq!(remote_workers[0].reconnecting, false);

    let third_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-c".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("third response");
    assert_eq!(third_response.status(), reqwest::StatusCode::OK);
    let third_thread: ThreadResponse = third_response.json().await.expect("third thread");
    assert_eq!(third_thread.thread.id, "thread-worker-a-2");

    drop(events_response);
    server.shutdown().await.expect("shutdown");
}
