use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_aggregates_apps_list_over_v2() {
    let worker_a = start_mock_remote_multi_connection_apps_list_server(vec![
        ("shared-app", "Shared App"),
        ("worker-a-app", "Worker A App"),
    ])
    .await;
    let worker_b = start_mock_remote_multi_connection_apps_list_server(vec![
        ("shared-app", "Shared App"),
        ("worker-b-app", "Worker B App"),
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

    let first_page: AppsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(1),
            params: AppsListParams {
                cursor: None,
                limit: Some(2),
                thread_id: None,
                force_refetch: false,
            },
        }),
    )
    .await
    .expect("app/list first page should finish in time")
    .expect("app/list first page should aggregate through multi-worker gateway");
    assert_eq!(
        first_page
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-app", "worker-a-app"]
    );
    assert_eq!(first_page.next_cursor.as_deref(), Some("apps-offset:2"));

    let second_page: AppsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(2),
            params: AppsListParams {
                cursor: first_page.next_cursor.clone(),
                limit: Some(2),
                thread_id: None,
                force_refetch: false,
            },
        }),
    )
    .await
    .expect("app/list second page should finish in time")
    .expect("app/list second page should aggregate through multi-worker gateway");
    assert_eq!(
        second_page
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-b-app"]
    );
    assert_eq!(second_page.next_cursor, None);

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
async fn remote_multi_worker_aggregates_mcp_server_status_list_over_v2() {
    let worker_a = start_mock_remote_multi_connection_mcp_server_status_list_server(vec![
        "shared-mcp",
        "worker-a-mcp",
    ])
    .await;
    let worker_b = start_mock_remote_multi_connection_mcp_server_status_list_server(vec![
        "shared-mcp",
        "worker-b-mcp",
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

    let first_page: ListMcpServerStatusResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(1),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: Some(2),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServerStatus/list first page should finish in time")
    .expect("mcpServerStatus/list first page should aggregate through multi-worker gateway");
    assert_eq!(
        first_page
            .data
            .iter()
            .map(|status| status.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-mcp", "worker-a-mcp"]
    );
    assert_eq!(
        first_page.next_cursor.as_deref(),
        Some("mcp-status-offset:2")
    );

    let second_page: ListMcpServerStatusResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(2),
            params: ListMcpServerStatusParams {
                cursor: first_page.next_cursor.clone(),
                limit: Some(2),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServerStatus/list second page should finish in time")
    .expect("mcpServerStatus/list second page should aggregate through multi-worker gateway");
    assert_eq!(
        second_page
            .data
            .iter()
            .map(|status| status.name.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-b-mcp"]
    );
    assert_eq!(second_page.next_cursor, None);

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
