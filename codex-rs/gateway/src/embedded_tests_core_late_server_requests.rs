use super::*;
use pretty_assertions::assert_eq;

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
