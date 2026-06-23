use super::*;
use pretty_assertions::assert_eq;

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
