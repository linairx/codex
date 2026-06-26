use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_mcp_oauth_login_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let model_server = start_mock_responses_server_repeating_assistant("unused").await;
    let (embedded_mcp_oauth_url, embedded_mcp_oauth_handle) =
        start_embedded_plugin_mcp_oauth_server()
            .await
            .expect("embedded MCP OAuth server should start");
    write_mock_responses_config_toml(codex_home.path(), &model_server.uri)
        .expect("config.toml should be written");
    let config_path = codex_home.path().join("config.toml");
    let mut config_toml =
        std::fs::read_to_string(&config_path).expect("config.toml should remain readable");
    config_toml.push_str(&format!(
        "\nmcp_oauth_credentials_store = \"file\"\n\n[mcp_servers.demo-mcp]\nurl = \"{embedded_mcp_oauth_url}/mcp\"\n"
    ));
    std::fs::write(&config_path, config_toml).expect("config.toml should be updated");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        vec![
            (
                "mcp_oauth_credentials_store".to_string(),
                toml::Value::String("file".to_string()),
            ),
            (
                "mcp_servers.demo-mcp.url".to_string(),
                toml::Value::String(format!("{embedded_mcp_oauth_url}/mcp")),
            ),
        ],
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
    .expect("remote client should connect to embedded gateway");

    let mcp_oauth_login: McpServerOauthLoginResponse = client
        .request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(1),
            params: McpServerOauthLoginParams {
                name: "demo-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
                thread_id: None,
            },
        })
        .await
        .expect("mcpServer/oauth/login should succeed through embedded gateway");
    assert!(
        mcp_oauth_login
            .authorization_url
            .starts_with(&embedded_mcp_oauth_url)
    );

    let oauth_browser_response = reqwest::get(&mcp_oauth_login.authorization_url)
        .await
        .expect("authorization URL should be reachable");
    assert_eq!(oauth_browser_response.status(), reqwest::StatusCode::OK);

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::McpServerOauthLoginCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/oauthLogin/completed notification should arrive");
    assert_eq!(mcp_oauth_completed.name, "demo-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    embedded_mcp_oauth_handle.abort();
    model_server.shutdown().await;
}
