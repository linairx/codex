use super::*;

#[path = "embedded_tests_remote_bootstrap_setup_discovery.rs"]
mod embedded_tests_remote_bootstrap_setup_discovery;

#[path = "embedded_tests_remote_bootstrap_setup_files.rs"]
mod embedded_tests_remote_bootstrap_setup_files;

#[path = "embedded_tests_remote_bootstrap_setup_account.rs"]
mod embedded_tests_remote_bootstrap_setup_account;

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_bootstrap_setup_methods() {
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to remote gateway");

    embedded_tests_remote_bootstrap_setup_discovery::assert_bootstrap_setup_discovery_and_mcp(
        &mut client,
    )
    .await;
    embedded_tests_remote_bootstrap_setup_files::assert_bootstrap_setup_plugins_files_and_search(
        &mut client,
    )
    .await;
    embedded_tests_remote_bootstrap_setup_account::assert_bootstrap_setup_account_and_command(
        &mut client,
    )
    .await;

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
