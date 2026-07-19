use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_account_rate_limits_read() {
    let codex_home = tempdir().expect("tempdir");
    let model_server = start_mock_responses_server_repeating_assistant("unused").await;
    let (apps_server_url, apps_server_handle) = start_embedded_gateway_apps_server()
        .await
        .expect("apps server should start");
    write_embedded_mcp_config_toml(codex_home.path(), &model_server.uri, &apps_server_url)
        .expect("config.toml should be written");
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .plan_type("pro"),
        AuthCredentialsStoreMode::File,
    )
    .expect("chatgpt auth should be written");
    let mut config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    config.chatgpt_base_url = apps_server_url.to_string();
    config.cli_auth_credentials_store_mode = AuthCredentialsStoreMode::File;
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
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to embedded gateway");

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(10),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(1),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should succeed through embedded gateway");
    assert_eq!(
        rate_limits,
        GetAccountRateLimitsResponse {
            rate_limits: RateLimitSnapshot {
                limit_id: Some("codex".to_string()),
                limit_name: None,
                primary: Some(RateLimitWindow {
                    used_percent: 42,
                    window_duration_mins: Some(60),
                    resets_at: Some(1_735_689_720),
                }),
                secondary: Some(RateLimitWindow {
                    used_percent: 5,
                    window_duration_mins: Some(1_440),
                    resets_at: Some(1_735_693_200),
                }),
                credits: None,
                plan_type: Some(AccountPlanType::Pro),
                rate_limit_reached_type: Some(
                    RateLimitReachedType::WorkspaceMemberUsageLimitReached,
                ),
                individual_limit: None,
                spend_control_reached: None,
            },
            rate_limits_by_limit_id: Some(
                [
                    (
                        "codex".to_string(),
                        RateLimitSnapshot {
                            limit_id: Some("codex".to_string()),
                            limit_name: None,
                            primary: Some(RateLimitWindow {
                                used_percent: 42,
                                window_duration_mins: Some(60),
                                resets_at: Some(1_735_689_720),
                            }),
                            secondary: Some(RateLimitWindow {
                                used_percent: 5,
                                window_duration_mins: Some(1_440),
                                resets_at: Some(1_735_693_200),
                            }),
                            credits: None,
                            plan_type: Some(AccountPlanType::Pro),
                            rate_limit_reached_type: Some(
                                RateLimitReachedType::WorkspaceMemberUsageLimitReached,
                            ),
                            individual_limit: None,
                            spend_control_reached: None,
                        },
                    ),
                    (
                        "codex_other".to_string(),
                        RateLimitSnapshot {
                            limit_id: Some("codex_other".to_string()),
                            limit_name: Some("codex_other".to_string()),
                            primary: Some(RateLimitWindow {
                                used_percent: 88,
                                window_duration_mins: Some(30),
                                resets_at: Some(1_735_693_200),
                            }),
                            secondary: None,
                            credits: None,
                            plan_type: Some(AccountPlanType::Pro),
                            rate_limit_reached_type: None,
                            individual_limit: None,
                            spend_control_reached: None,
                        },
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            rate_limit_reset_credits: None,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    apps_server_handle.abort();
    let _ = apps_server_handle.await;
    model_server.shutdown().await;
}
