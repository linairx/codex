use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_forwards_mcp_startup_status_notifications_over_v2() {
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
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to remote gateway");

    let startup_status = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::McpServerStatusUpdated(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/startupStatus/updated notification should arrive");
    assert_eq!(startup_status.name, "calendar-mcp");
    assert_eq!(startup_status.status, McpServerStartupState::Ready);
    assert_eq!(startup_status.error, None);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_forwards_skills_changed_notifications_over_v2() {
    let websocket_url = start_mock_remote_multi_connection_skills_changed_server().await;
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

    let first_notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("initial skills/changed should finish in time")
        .expect("event stream should stay open");
    assert!(matches!(
        first_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));

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
    .expect("skills/list should succeed through remote gateway");

    let second_notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time")
        .expect("event stream should stay open");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
