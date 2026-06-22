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

#[tokio::test]
async fn embedded_server_supports_server_request_roundtrip_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let model_server = start_mock_responses_server_sequence(vec![
        mock_responses_request_user_input_sse_body("call1"),
        mock_responses_sse_body("done"),
    ])
    .await;
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
    .expect("remote client should connect to embedded gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                model_provider: None,
                service_tier: None,
                cwd: Some(codex_home.path().display().to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "ask something".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: Some("mock-model".to_string()),
                service_tier: None,
                effort: Some(ReasoningEffort::Medium),
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: "mock-model".to_string(),
                        reasoning_effort: Some(ReasoningEffort::Medium),
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();

    let (request_id, params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("server request should arrive");
    assert_eq!(params.thread_id, started.thread.id);
    assert_eq!(params.turn_id, turn_id);
    assert_eq!(params.item_id, "call1");
    assert_eq!(params.questions.len(), 1);
    let resolved_request_id = request_id.clone();

    let health_client = reqwest::Client::new();
    let healthz_response = health_client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.v2_connections.active_connection_count, 1);
    assert_eq!(
        health
            .v2_connections
            .active_connection_pending_server_request_count,
        1
    );
    assert_eq!(
        health
            .v2_connections
            .active_connection_answered_but_unresolved_server_request_count,
        0
    );

    let mut answers = HashMap::new();
    answers.insert(
        "confirm_path".to_string(),
        ToolRequestUserInputAnswer {
            answers: vec!["yes".to_string()],
        },
    );
    client
        .resolve_server_request(
            request_id,
            serde_json::to_value(ToolRequestUserInputResponse { answers })
                .expect("server request response should serialize"),
        )
        .await
        .expect("server request should resolve");

    let mut saw_resolved = false;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(notification) = event {
                match notification {
                    ServerNotification::ServerRequestResolved(
                        ServerRequestResolvedNotification {
                            thread_id,
                            request_id,
                        },
                    ) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(request_id, resolved_request_id);
                        saw_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_resolved, true,
                            "serverRequest/resolved should arrive first"
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("resolved notification and turn completion should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_supports_permissions_server_request_roundtrip_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let model_server = start_mock_responses_server_sequence(vec![
            format!(
                "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"function_call\",\"id\":\"call1\",\"call_id\":\"call1\",\"name\":\"request_permissions\",\"arguments\":\"{{\\\"reason\\\":\\\"Select a workspace root\\\",\\\"permissions\\\":{{\\\"file_system\\\":{{\\\"write\\\":[\\\".\\\",\\\"../shared\\\"]}}}}}}\"}}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"function_call\",\"id\":\"call1\",\"call_id\":\"call1\",\"name\":\"request_permissions\",\"arguments\":\"{{\\\"reason\\\":\\\"Select a workspace root\\\",\\\"permissions\\\":{{\\\"file_system\\\":{{\\\"write\\\":[\\\".\\\",\\\"../shared\\\"]}}}}}}\"}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
            ),
            mock_responses_sse_body("done"),
        ])
        .await;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
experimental_bearer_token = "sk-test-key"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[features]
request_permissions_tool = true
"#,
            model_server.uri
        ),
    )
    .expect("config.toml should be written");
    std::fs::write(
        codex_home.path().join("auth.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "OPENAI_API_KEY": "sk-test-key",
            "tokens": serde_json::Value::Null,
            "last_refresh": serde_json::Value::Null,
        }))
        .expect("auth.json should serialize"),
    )
    .expect("auth.json should be written");
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to embedded gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                model_provider: None,
                service_tier: None,
                cwd: Some(codex_home.path().display().to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "pick a directory".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: Some("mock-model".to_string()),
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();

    let (request_id, params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("permissions server request should arrive");
    assert_eq!(params.thread_id, started.thread.id);
    assert_eq!(params.turn_id, turn_id);
    assert_eq!(params.item_id, "call1");
    assert_eq!(params.reason, Some("Select a workspace root".to_string()));
    let requested_writes = params
        .permissions
        .file_system
        .and_then(|file_system| file_system.write)
        .expect("request should include write permissions");
    assert_eq!(requested_writes.len(), 2);
    let resolved_request_id = request_id.clone();

    client
        .resolve_server_request(
            request_id,
            serde_json::to_value(PermissionsRequestApprovalResponse {
                strict_auto_review: None,

                permissions: codex_app_server_protocol::GrantedPermissionProfile {
                    network: None,
                    file_system: Some(codex_app_server_protocol::AdditionalFileSystemPermissions {
                        read: None,
                        write: Some(vec![requested_writes[0].clone()]),
                        glob_scan_max_depth: None,
                        entries: None,
                    }),
                },
                scope: PermissionGrantScope::Turn,
            })
            .expect("permissions response should serialize"),
        )
        .await
        .expect("permissions request should resolve");

    let mut saw_resolved = false;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(notification) = event {
                match notification {
                    ServerNotification::ServerRequestResolved(
                        ServerRequestResolvedNotification {
                            thread_id,
                            request_id,
                        },
                    ) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(request_id, resolved_request_id);
                        saw_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_resolved, true,
                            "serverRequest/resolved should arrive first"
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("resolved notification and turn completion should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}
