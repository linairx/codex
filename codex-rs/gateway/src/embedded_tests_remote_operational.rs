use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_forwards_config_warning_notifications_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let mut config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    config
        .startup_warnings
        .push("Gateway embedded config warning".to_string());

    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
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
    .expect("remote client should connect to embedded gateway");

    let config_warning = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ConfigWarning(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("configWarning notification should arrive");
    assert_eq!(config_warning.summary, "Gateway embedded config warning");
    assert_eq!(config_warning.details, None);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_server_forwards_thread_creation_requests() {
    let websocket_url = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-remote",
        "/tmp/project",
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
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url,
                    auth_token: Some("secret-token".to_string()),
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
    let response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("http response");

    let status = response.status();
    let body_text = response.text().await.expect("response text");
    assert_eq!(status, reqwest::StatusCode::OK, "{body_text}");
    let body: ThreadResponse = serde_json::from_str(&body_text).expect("thread response");
    assert_eq!(body.thread.id, "thread-remote");
    assert_eq!(body.thread.preview, "/tmp/project");

    server.shutdown().await.expect("shutdown");
}
