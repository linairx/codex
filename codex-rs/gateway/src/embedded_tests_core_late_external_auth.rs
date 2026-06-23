use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_external_auth_onboarding_flows_over_v2() {
    let codex_home = tempdir().expect("tempdir");
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

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("embedded@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-embedded"),
    )
    .expect("access token should encode");

    let login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptAuthTokens {
                access_token,
                chatgpt_account_id: "org-embedded".to_string(),
                chatgpt_plan_type: Some("pro".to_string()),
            },
        })
        .await
        .expect("account/login/start should succeed through embedded gateway");
    assert_eq!(login, LoginAccountResponse::ChatgptAuthTokens {});

    let login_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/login/completed notification should arrive");
    assert_eq!(login_completed.login_id, None);
    assert_eq!(login_completed.success, true);
    assert_eq!(login_completed.error, None);

    let account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated notification should arrive");
    assert_eq!(account_updated.auth_mode, Some(AuthMode::ChatgptAuthTokens));
    assert_eq!(account_updated.plan_type, Some(AccountPlanType::Pro));

    let cancel_login: CancelLoginAccountResponse = client
        .request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(2),
            params: CancelLoginAccountParams {
                login_id: "00000000-0000-0000-0000-000000000001".to_string(),
            },
        })
        .await
        .expect("account/login/cancel should succeed through embedded gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::NotFound);

    let account: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(3),
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read should succeed through embedded gateway after login");
    assert_eq!(account.account.is_some(), true);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_forwards_external_auth_tokens_to_model_requests_over_v2() {
    let model_server = start_mock_responses_server_repeating_assistant("turn ok").await;
    let codex_home = tempdir().expect("tempdir");
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
requires_openai_auth = true
"#,
            model_server.uri
        ),
    )
    .expect("config.toml should be written");
    write_models_cache(codex_home.path()).expect("models cache should be written");
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

    let initial_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("initial@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-initial"),
    )
    .expect("initial access token should encode");
    let login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptAuthTokens {
                access_token: initial_access_token.clone(),
                chatgpt_account_id: "org-initial".to_string(),
                chatgpt_plan_type: Some("pro".to_string()),
            },
        })
        .await
        .expect("account/login/start should succeed through embedded gateway");
    assert_eq!(login, LoginAccountResponse::ChatgptAuthTokens {});

    let account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated notification should arrive");
    assert_eq!(account_updated.auth_mode, Some(AuthMode::ChatgptAuthTokens));
    assert_eq!(account_updated.plan_type, Some(AccountPlanType::Pro));

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Hello".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    assert_eq!(turn_started.turn.status, TurnStatus::InProgress);

    let request = timeout(Duration::from_secs(30), model_server.wait_for_request(0))
        .await
        .expect("responses request should arrive");
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {initial_access_token}"))
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}
