use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_realtime_list_voices() {
    let model_server = start_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = tempdir().expect("tempdir");
    std::fs::create_dir_all(codex_home.path().join(".git")).expect(".git should be created");
    write_mock_responses_config_toml(codex_home.path(), &model_server.uri)
        .expect("config.toml should be written");
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
    .expect("remote client should connect to embedded gateway");

    let voices: ThreadRealtimeListVoicesResponse = client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed through embedded gateway");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList::builtin(),
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}
