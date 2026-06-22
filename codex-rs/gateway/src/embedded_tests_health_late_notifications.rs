use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_deduplicates_external_agent_import_completed_notifications_after_worker_reconnect()
 {
    let (worker_a, _worker_a_requests) =
        start_reconnecting_v2_multi_connection_session_mutation_server("worker-a").await;
    let (worker_b, _worker_b_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-b").await;
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
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
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before same-session import-completed test starts");

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

    sleep(Duration::from_millis(200)).await;

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should reconnect and fan out");
    assert!(!imported.import_id.is_empty());

    let notification = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("externalAgentConfig/import/completed should finish in time after reconnect")
        .expect("event stream should stay open after worker reconnect");
    assert!(matches!(
        notification,
        AppServerEvent::ServerNotification(ServerNotification::ExternalAgentConfigImportCompleted(
            _
        ))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate externalAgentConfig/import/completed should still be suppressed after reconnect"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_for_plugin_management() {
    let worker_a = start_reconnecting_v2_multi_connection_plugin_server(
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
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
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before same-session plugin test starts");

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

    sleep(Duration::from_millis(200)).await;

    let listed: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(1),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/shared-repo"],
            }))
            .expect("plugin/list params should deserialize"),
        })
        .await
        .expect("plugin/list should re-add the recovered worker");
    assert_eq!(listed.marketplaces.len(), 2);
    assert_eq!(
        listed
            .marketplaces
            .iter()
            .map(|marketplace| marketplace.name.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-a-marketplace", "worker-b-marketplace"]
    );

    let plugin: PluginReadResponse = client
        .request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(2),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/worker-a/marketplace.json",
                "pluginName": "worker-a-plugin",
            }))
            .expect("plugin/read params should deserialize"),
        })
        .await
        .expect("plugin/read should route to the re-added worker");
    assert_eq!(plugin.plugin.marketplace_name, "worker-a-marketplace");
    assert_eq!(plugin.plugin.summary.name, "worker-a-plugin");
    assert_eq!(
        plugin.plugin.description.as_deref(),
        Some("Worker A plugin detail")
    );

    let install: PluginInstallResponse = client
        .request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(3),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/worker-a/marketplace.json",
                "pluginName": "worker-a-plugin",
            }))
            .expect("plugin/install params should deserialize"),
        })
        .await
        .expect("plugin/install should route to the re-added worker");
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
        .expect("plugin/list after install should still include recovered worker state");
    assert_eq!(listed_after_install.marketplaces.len(), 2);
    assert_eq!(
        listed_after_install
            .marketplaces
            .iter()
            .find(|marketplace| marketplace.name == "worker-a-marketplace")
            .expect("worker-a marketplace should exist")
            .plugins[0]
            .installed,
        true
    );
    assert_eq!(
        listed_after_install
            .marketplaces
            .iter()
            .find(|marketplace| marketplace.name == "worker-b-marketplace")
            .expect("worker-b marketplace should exist")
            .plugins[0]
            .installed,
        false
    );

    let uninstall: PluginUninstallResponse = client
        .request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(5),
            params: PluginUninstallParams {
                plugin_id: "worker-a-plugin@worker-a-marketplace".to_string(),
            },
        })
        .await
        .expect("plugin/uninstall should route to the re-added worker");
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
        .expect("plugin/list after uninstall should keep recovered worker state");
    assert_eq!(listed_after_uninstall.marketplaces.len(), 2);
    assert_eq!(
        listed_after_uninstall
            .marketplaces
            .iter()
            .find(|marketplace| marketplace.name == "worker-a-marketplace")
            .expect("worker-a marketplace should exist")
            .plugins[0]
            .installed,
        false
    );
    assert_eq!(
        listed_after_uninstall
            .marketplaces
            .iter()
            .find(|marketplace| marketplace.name == "worker-b-marketplace")
            .expect("worker-b marketplace should exist")
            .plugins[0]
            .installed,
        false
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
