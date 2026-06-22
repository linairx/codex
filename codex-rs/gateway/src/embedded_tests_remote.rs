use super::*;
use pretty_assertions::assert_eq;

const EMBEDDED_TOOL_NAMESPACE: &str = "mcp__codex_apps__calendar";
const EMBEDDED_CALLABLE_TOOL_NAME: &str = "_confirm_action";
const EMBEDDED_TOOL_CALL_ID: &str = "call-calendar-confirm";

#[tokio::test]
async fn embedded_server_supports_command_and_file_approval_roundtrips_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let workspace = codex_home.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace should be created");
    let patch = r#"*** Begin Patch
*** Add File: README.md
+new line
*** End Patch
"#;
    let model_server = start_mock_responses_server_sequence(vec![
        create_exec_command_sse_response("exec-call")
            .expect("exec command response should serialize"),
        mock_responses_sse_body("command complete"),
        create_apply_patch_sse_response(patch, "patch-call")
            .expect("apply patch response should serialize"),
        mock_responses_sse_body("patch complete"),
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
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            model_server.uri
        ),
    )
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
                cwd: Some(workspace.display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let command_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "run a command".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("command turn/start should succeed through embedded gateway");
    let command_turn_id = command_turn.turn.id.clone();

    let (command_request_id, command_params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("command approval request should arrive");
    assert_eq!(command_params.thread_id, started.thread.id);
    assert_eq!(command_params.turn_id, command_turn_id);
    assert_eq!(command_params.item_id, "exec-call");
    let resolved_command_request_id = command_request_id.clone();

    client
        .resolve_server_request(
            command_request_id,
            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Accept,
            })
            .expect("command approval response should serialize"),
        )
        .await
        .expect("command approval request should resolve");

    let mut saw_command_resolved = false;
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
                        assert_eq!(request_id, resolved_command_request_id);
                        saw_command_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, command_turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_command_resolved, true,
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
    .expect("command turn should resolve and complete");

    let file_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "apply patch".to_string(),
                    text_elements: Vec::new(),
                }],
                cwd: Some(workspace.clone()),
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("file turn/start should succeed through embedded gateway");
    let file_turn_id = file_turn.turn.id.clone();

    let (file_request_id, file_params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("file approval request should arrive");
    assert_eq!(file_params.thread_id, started.thread.id);
    assert_eq!(file_params.turn_id, file_turn_id);
    assert_eq!(file_params.item_id, "patch-call");
    let resolved_file_request_id = file_request_id.clone();

    client
        .resolve_server_request(
            file_request_id,
            serde_json::to_value(FileChangeRequestApprovalResponse {
                decision: FileChangeApprovalDecision::Accept,
            })
            .expect("file approval response should serialize"),
        )
        .await
        .expect("file approval request should resolve");

    let mut saw_file_resolved = false;
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
                        assert_eq!(request_id, resolved_file_request_id);
                        saw_file_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, file_turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_file_resolved, true,
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
    .expect("file turn should resolve and complete");

    assert_eq!(
        std::fs::read_to_string(workspace.join("README.md"))
            .expect("README should be written after accepting patch"),
        "new line\n"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_supports_mcp_elicitation_server_request_roundtrip_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let model_server = start_mock_responses_server_sequence(vec![
        mock_responses_sse_body("Warmup"),
        mock_responses_namespaced_function_call_sse_body(
            EMBEDDED_TOOL_CALL_ID,
            EMBEDDED_TOOL_NAMESPACE,
            EMBEDDED_CALLABLE_TOOL_NAME,
            "{}",
        ),
        mock_responses_sse_body("Done"),
    ])
    .await;
    let (apps_server_url, apps_server_handle) = start_embedded_gateway_apps_server()
        .await
        .expect("apps server");
    write_embedded_mcp_config_toml(codex_home.path(), &model_server.uri, &apps_server_url)
        .expect("config.toml should be written");
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )
    .expect("chatgpt auth should be written");
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
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let _: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Warm up connectors.".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("warmup turn/start should succeed through embedded gateway");

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                completed,
            )) = event
            {
                assert_eq!(completed.thread_id, started.thread.id);
                assert_eq!(completed.turn.status, TurnStatus::Completed);
                break;
            }
        }
    })
    .await
    .expect("warmup completion should arrive");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Use [$calendar](app://calendar) to run the calendar tool.".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
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
            if let AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("mcp elicitation request should arrive");
    let requested_schema: McpElicitationSchema = serde_json::from_value(
        serde_json::to_value(
            ElicitationSchema::builder()
                .required_property("confirmed", PrimitiveSchema::Boolean(BooleanSchema::new()))
                .build()
                .expect("schema should build"),
        )
        .expect("schema should serialize"),
    )
    .expect("schema should decode");
    assert_eq!(params.thread_id, started.thread.id);
    assert_eq!(params.turn_id, Some(turn_id.clone()));
    assert_eq!(params.server_name, "codex_apps");
    assert_eq!(
        params.request,
        McpServerElicitationRequest::Form {
            meta: None,
            message: EMBEDDED_ELICITATION_MESSAGE.to_string(),
            requested_schema,
        }
    );
    let resolved_request_id = request_id.clone();

    client
        .resolve_server_request(
            request_id,
            serde_json::to_value(McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Accept,
                content: Some(serde_json::json!({
                    "confirmed": true,
                })),
                meta: None,
            })
            .expect("elicitation response should serialize"),
        )
        .await
        .expect("mcp elicitation request should resolve");

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
    apps_server_handle.abort();
    let _ = apps_server_handle.await;
}

#[tokio::test]
async fn embedded_server_supports_dynamic_tool_call_server_request_roundtrip_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let tool_name = "demo_tool";
    let call_id = "dyn-call-embedded";
    let tool_arguments = serde_json::json!({
        "city": "Paris",
    });
    let tool_call_arguments =
        serde_json::to_string(&tool_arguments).expect("tool arguments should serialize");
    let model_server = start_mock_responses_server_sequence(vec![
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(call_id, tool_name, &tool_call_arguments),
            responses::ev_completed("resp-1"),
        ]),
        mock_responses_sse_body("Done"),
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
                dynamic_tools: Some(vec![DynamicToolSpec::Function(DynamicToolFunctionSpec {
                    name: tool_name.to_string(),
                    description: "Demo dynamic tool".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" },
                        },
                        "required": ["city"],
                        "additionalProperties": false,
                    }),
                    defer_loading: false,
                })]),
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
                    text: "Run the dynamic tool".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();

    let mut saw_item_started = false;
    let mut saw_item_completed = false;
    let mut saw_turn_completed = false;
    let mut resolved_request_id: Option<RequestId> = None;
    let dynamic_tool_result = timeout(Duration::from_secs(10), async {
        while !(saw_item_started && saw_item_completed && saw_turn_completed) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerRequest(ServerRequest::DynamicToolCall {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, started.thread.id);
                    assert_eq!(params.turn_id, turn_id);
                    assert_eq!(params.call_id, call_id);
                    assert_eq!(params.tool, tool_name);
                    assert_eq!(params.arguments, tool_arguments);
                    resolved_request_id = Some(request_id.clone());
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: "dynamic-ok".to_string(),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool call response should serialize"),
                        )
                        .await
                        .expect("dynamic tool call should resolve");
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                                &notification.item,
                                ThreadItem::DynamicToolCall {
                    namespace: None,
                                    id: item_call_id,
                                    tool: item_tool_name,
                                    status,
                                    ..
                                } if item_call_id == call_id
                                    && item_tool_name == tool_name
                                    && *status == DynamicToolCallStatus::InProgress
                            ) =>
                {
                    saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                                &notification.item,
                                ThreadItem::DynamicToolCall {
                    namespace: None,
                                    id: item_call_id,
                                    tool: item_tool_name,
                                    status,
                                    ..
                                } if item_call_id == call_id
                                    && item_tool_name == tool_name
                                    && *status == DynamicToolCallStatus::Completed
                            ) =>
                {
                    saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    ServerRequestResolvedNotification {
                        thread_id,
                        request_id,
                    },
                )) => {
                    assert_eq!(thread_id, started.thread.id);
                    assert_eq!(resolved_request_id.as_ref(), Some(&request_id));
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_id =>
                {
                    assert_eq!(notification.turn.status, TurnStatus::Completed);
                    saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        dynamic_tool_result.is_ok(),
        true,
        "dynamic tool call flow should complete: started={saw_item_started} completed={saw_item_completed} turn_completed={saw_turn_completed}"
    );
    assert_eq!(saw_item_started, true);
    assert_eq!(saw_item_completed, true);
    assert_eq!(saw_turn_completed, true);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_turn_workflow() {
    let model_server = start_mock_responses_server_sequence(vec![responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_message_item_added("msg-1", ""),
        responses::ev_reasoning_item_added("reasoning-1", &[""]),
        serde_json::json!({
            "type": "response.reasoning_summary_part.added",
            "summary_index": 0,
        }),
        responses::ev_output_text_delta("Done"),
        responses::ev_reasoning_summary_text_delta("embedded summary"),
        responses::ev_reasoning_text_delta("embedded reasoning"),
        responses::ev_assistant_message("msg-1", "Done"),
        responses::ev_reasoning_item(
            "reasoning-1",
            &["embedded summary"],
            &["embedded reasoning"],
        ),
        responses::ev_completed_with_tokens("resp-1", /*total_tokens*/ 42),
    ])])
    .await;
    let codex_home = tempdir().expect("tempdir");
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
                model: None,
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
                    text: "hello from embedded gateway".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
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
    assert_eq!(turn_started_response.turn.status, TurnStatus::InProgress);

    let turn_id = turn_started_response.turn.id.clone();
    let mut saw_thread_active = false;
    let mut saw_turn_started = false;
    let mut saw_item_started = false;
    let mut saw_agent_delta = false;
    let mut saw_reasoning_summary_part_added = false;
    let mut saw_reasoning_summary_delta = false;
    let mut saw_reasoning_text_delta = false;
    let mut saw_thread_token_usage = false;
    let mut saw_item_completed = false;
    let mut saw_turn_completed = false;

    timeout(Duration::from_secs(10), async {
        while !(saw_thread_active
            && saw_turn_started
            && saw_item_started
            && saw_agent_delta
            && saw_reasoning_summary_part_added
            && saw_reasoning_summary_delta
            && saw_reasoning_text_delta
            && saw_thread_token_usage
            && saw_item_completed
            && saw_turn_completed)
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && matches!(notification.status, ThreadStatus::Active { .. }) =>
                {
                    saw_thread_active = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    TurnStartedNotification { thread_id, turn },
                )) if thread_id == started.thread.id && turn.id == turn_id => {
                    saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-1" && text.is_empty()
                    ) =>
                {
                    saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && notification.delta == "Done" =>
                {
                    saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && notification.item_id == "reasoning-1"
                    && notification.summary_index == 0 =>
                {
                    saw_reasoning_summary_part_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && notification.delta == "embedded summary"
                    && notification.summary_index == 0 =>
                {
                    saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && notification.delta == "embedded reasoning"
                    && notification.content_index == 0 =>
                {
                    saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && notification.token_usage.total.total_tokens == 42
                    && notification.token_usage.last.total_tokens == 42 =>
                {
                    saw_thread_token_usage = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-1" && text == "Done"
                    ) =>
                {
                    saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    TurnCompletedNotification { thread_id, turn },
                )) if thread_id == started.thread.id
                    && turn.id == turn_id
                    && turn.status == TurnStatus::Completed =>
                {
                    assert_eq!(
                        saw_item_started, true,
                        "agent message item should start before turn completion"
                    );
                    assert_eq!(
                        saw_item_completed, true,
                        "agent message item should complete before turn completion"
                    );
                    assert_eq!(
                        saw_reasoning_summary_delta, true,
                        "reasoning summary delta should arrive before turn completion"
                    );
                    assert_eq!(
                        saw_reasoning_text_delta, true,
                        "reasoning text delta should arrive before turn completion"
                    );
                    saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("turn lifecycle notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_plan_item_workflow() {
    let full_message = "Intro\n<proposed_plan>\n- Step 1\n</proposed_plan>\nOutro";
    let model_server = start_mock_responses_server_sequence(vec![responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_message_item_added("msg-1", ""),
        responses::ev_output_text_delta(full_message),
        responses::ev_assistant_message("msg-1", full_message),
        responses::ev_completed("resp-1"),
    ])])
    .await;
    let codex_home = tempdir().expect("tempdir");
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
                model: None,
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
                    text: "make a plan".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: "mock-model".to_string(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();
    let expected_plan_item_id = format!("{turn_id}-plan");
    let mut saw_plan_started = false;
    let mut saw_plan_delta = false;
    let mut saw_plan_completed = false;
    let mut saw_turn_completed = false;

    timeout(Duration::from_secs(10), async {
        while !(saw_plan_started && saw_plan_delta && saw_plan_completed && saw_turn_completed) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == &expected_plan_item_id
                            && text.is_empty()
                    ) =>
                {
                    saw_plan_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification))
                    if notification.thread_id == started.thread.id
                        && notification.turn_id == turn_id
                        && notification.item_id == expected_plan_item_id
                        && notification.delta == "- Step 1\n" =>
                {
                    saw_plan_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == &expected_plan_item_id
                            && text == "- Step 1\n"
                    ) =>
                {
                    saw_plan_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_id
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("plan item notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_plan_item_workflow() {
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

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "make a remote plan".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: "mock-model".to_string(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through remote gateway");
    let turn_id = turn_started_response.turn.id.clone();
    let mut saw_plan_started = false;
    let mut saw_plan_completed = false;
    let mut saw_turn_completed = false;

    timeout(Duration::from_secs(10), async {
        while !(saw_plan_started && saw_plan_completed && saw_turn_completed) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == "plan-remote-workflow"
                            && text.is_empty()
                    ) =>
                {
                    saw_plan_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == "plan-remote-workflow"
                            && text == "- Remote step\n"
                    ) =>
                {
                    saw_plan_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_id
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("remote plan item notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_drop_in_v2_client_plan_item_workflow() {
    let worker_a = start_mock_remote_workflow_server().await;
    let worker_b =
        start_mock_remote_workflow_server_with_thread_id("thread-remote-workflow-b").await;
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
                        websocket_url: worker_a.clone(),

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
    .expect("remote client should connect to multi-worker gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through multi-worker gateway");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "make a multi-worker plan".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: "mock-model".to_string(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should route to the owning worker through multi-worker gateway");
    let turn_id = turn_started_response.turn.id.clone();
    let mut saw_plan_started = false;
    let mut saw_plan_completed = false;
    let mut saw_turn_completed = false;

    timeout(Duration::from_secs(5), async {
        while !(saw_plan_started && saw_plan_completed && saw_turn_completed) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == "plan-remote-workflow"
                            && text.is_empty()
                    ) =>
                {
                    saw_plan_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_id
                    && matches!(
                        &notification.item,
                        ThreadItem::Plan { id, text } if id == "plan-remote-workflow"
                            && text == "- Remote step\n"
                    ) =>
                {
                    saw_plan_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_id
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("multi-worker plan item notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_thread_control_workflow() {
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

    let unsubscribe_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("unsubscribe thread/start should succeed through embedded gateway");

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: unsubscribe_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed through embedded gateway");
    assert!(
        matches!(
            unsubscribe.status,
            ThreadUnsubscribeStatus::Unsubscribed | ThreadUnsubscribeStatus::NotSubscribed
        ),
        "unexpected unsubscribe status: {:?}",
        unsubscribe.status
    );

    let shell_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("shell thread/start should succeed through embedded gateway");

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(4),
            params: ThreadShellCommandParams {
                thread_id: shell_thread.thread.id.clone(),
                command: "printf embedded-gateway".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed through embedded gateway");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(5),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: shell_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed through embedded gateway");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let archive_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(6),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("archive thread/start should succeed through embedded gateway");

    let archive_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(7),
            params: TurnStartParams {
                thread_id: archive_thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "produce one turn".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("archive turn/start should succeed through embedded gateway");
    let archive_turn_id = archive_turn.turn.id.clone();

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == archive_thread.thread.id
                && turn.id == archive_turn_id
                && turn.status == TurnStatus::Completed
            {
                break;
            }
        }
    })
    .await
    .expect("archive seed turn should complete");

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(8),
            params: ThreadArchiveParams {
                thread_id: archive_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed through embedded gateway");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(9),
            params: ThreadUnarchiveParams {
                thread_id: archive_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed through embedded gateway");
    assert_eq!(unarchive.thread.id, archive_thread.thread.id);

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: archive_thread.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed through embedded gateway");
    assert_eq!(metadata_update.thread.id, archive_thread.thread.id);

    let rollback_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(11),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("rollback thread/start should succeed through embedded gateway");

    let rollback_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(12),
            params: TurnStartParams {
                thread_id: rollback_thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "produce one turn".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("rollback turn/start should succeed through embedded gateway");
    let rollback_turn_id = rollback_turn.turn.id.clone();

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == rollback_thread.thread.id
                && turn.id == rollback_turn_id
                && turn.status == TurnStatus::Completed
            {
                break;
            }
        }
    })
    .await
    .expect("rollback seed turn should complete");

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(13),
            params: ThreadTurnsListParams {
                thread_id: rollback_thread.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed through embedded gateway");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, rollback_turn_id);
    assert_eq!(turns.next_cursor, None);

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(14),
            params: ThreadRollbackParams {
                thread_id: rollback_thread.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed through embedded gateway");
    assert_eq!(rollback.thread.id, rollback_thread.thread.id);
    assert_eq!(rollback.thread.turns, Vec::new());

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(15),
            params: ThreadIncrementElicitationParams {
                thread_id: rollback_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed through embedded gateway");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadDecrementElicitationParams {
                thread_id: rollback_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed through embedded gateway");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(17),
            params: ThreadInjectItemsParams {
                thread_id: rollback_thread.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed through embedded gateway");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let compact_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(18),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("compact thread/start should succeed through embedded gateway");

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: compact_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed through embedded gateway");
    assert_eq!(compact, ThreadCompactStartResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

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

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_realtime_request_workflow() {
    let body = mock_responses_sse_body("Done");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let recorded_requests = requests.clone();
    let model_server_task = tokio::spawn(async move {
        let app = axum::Router::new()
            .route(
                "/v1/responses",
                axum::routing::post(move |headers: HeaderMap| {
                    let body = body.clone();
                    let requests = recorded_requests.clone();
                    async move {
                        requests.lock().await.push(MockResponsesRequest { headers });
                        (
                            [
                                (axum::http::header::CONTENT_TYPE, "text/event-stream"),
                                (axum::http::header::CACHE_CONTROL, "no-cache"),
                            ],
                            body,
                        )
                    }
                }),
            )
            .route(
                "/v1/realtime/calls",
                axum::routing::post(|| async move {
                    (
                        StatusCode::OK,
                        [(
                            axum::http::header::LOCATION,
                            "/v1/realtime/calls/rtc_app_test",
                        )],
                        "v=answer\r\n",
                    )
                }),
            );
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    let model_server = MockResponsesServer {
        uri: format!("http://{addr}"),
        requests,
        shutdown: Some(shutdown_tx),
        task: model_server_task,
    };
    let realtime_server = start_websocket_server(vec![vec![
        vec![serde_json::json!({
            "type": "session.updated",
            "session": { "id": "sess-embedded", "instructions": "backend prompt" }
        })],
        vec![],
        vec![
            serde_json::json!({
                "type": "response.output_audio.delta",
                "delta": "AQID",
                "sample_rate": 24_000,
                "channels": 1,
                "samples_per_channel": 512
            }),
            serde_json::json!({
                "type": "conversation.item.added",
                "item": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "hi" }]
                }
            }),
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.delta",
                "delta": "delegate now"
            }),
            serde_json::json!({
                "type": "response.output_text.delta",
                "delta": "working"
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.done",
                "transcript": "working on it"
            }),
            serde_json::json!({
                "type": "error",
                "message": "upstream boom"
            }),
        ],
        vec![],
    ]])
    .await;

    let codex_home = tempdir().expect("tempdir");
    std::fs::create_dir_all(codex_home.path().join(".git")).expect(".git should be created");
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
cli_auth_credentials_store = "file"
model_provider = "mock_provider"
experimental_realtime_ws_base_url = "{}"
experimental_realtime_ws_backend_prompt = "backend prompt"

[realtime]
version = "v2"
type = "conversational"

[features]
realtime_conversation = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            realtime_server.uri(),
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

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(true),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let realtime_started: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Audio,
                prompt: None,
                realtime_session_id: None,
                transport: Some(ThreadRealtimeStartTransport::Webrtc {
                    sdp: "v=offer\r\n".to_string(),
                }),
                voice: None,
                client_managed_handoffs: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("thread/realtime/start should succeed through embedded gateway");
    assert_eq!(realtime_started, ThreadRealtimeStartResponse {});

    timeout(Duration::from_secs(30), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
            {
                break;
            }
        }
    })
    .await
    .expect("thread/realtime/started should arrive");

    let session_update = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 0),
    )
    .await
    .expect("session.update should reach realtime sideband");
    assert_eq!(
        session_update.body_json()["type"],
        serde_json::json!("session.update")
    );

    let append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "BQYH".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(480),
                    item_id: None,
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should succeed through embedded gateway");
    assert_eq!(append_audio, ThreadRealtimeAppendAudioResponse {});

    let append_text: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendTextParams {
                thread_id: started.thread.id.clone(),
                text: "hello realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should succeed through embedded gateway");
    assert_eq!(append_text, ThreadRealtimeAppendTextResponse {});

    let stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeStopParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should succeed through embedded gateway");
    assert_eq!(stop_response, ThreadRealtimeStopResponse {});

    let first_append_request = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 1),
    )
    .await
    .expect("first realtime append request should reach sideband");
    let second_append_request = timeout(
        Duration::from_secs(5),
        realtime_server.wait_for_request(0, 2),
    )
    .await
    .expect("second realtime append request should reach sideband");
    let mut append_requests = [
        first_append_request.body_json(),
        second_append_request.body_json(),
    ];
    append_requests.sort_by(|left, right| left["type"].as_str().cmp(&right["type"].as_str()));
    assert_eq!(
        append_requests[0]["type"],
        serde_json::json!("conversation.item.create")
    );
    assert_eq!(
        append_requests[0]["item"]["content"][0]["text"],
        serde_json::json!("hello realtime")
    );
    assert_eq!(
        append_requests[1]["type"],
        serde_json::json!("input_audio_buffer.append")
    );

    let mut coverage = RealtimeStreamingCoverage {
        saw_started: true,
        ..Default::default()
    };
    let realtime_result = timeout(Duration::from_secs(5), async {
        while coverage
            != (RealtimeStreamingCoverage {
                saw_started: true,
                saw_item_added: false,
                saw_output_audio_delta: false,
                saw_transcript_delta: false,
                saw_transcript_done: false,
                saw_sdp: false,
                saw_error: false,
                saw_closed: false,
            })
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.delta == "working" =>
                {
                    coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.text == "working on it" =>
                {
                    coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.audio.data == "AQID"
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1 =>
                {
                    coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.message == "realtime transport warning" =>
                {
                    coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification
                        .reason
                        .as_deref()
                        .is_some_and(|reason| reason == "client requested stop") =>
                {
                    coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        realtime_result.is_ok(),
        true,
        "embedded realtime notifications should arrive: {coverage:?}"
    );

    assert_eq!(
        coverage,
        RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: false,
            saw_output_audio_delta: false,
            saw_transcript_delta: false,
            saw_transcript_done: false,
            saw_sdp: false,
            saw_error: false,
            saw_closed: false,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    realtime_server.shutdown().await;
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_turn_control_workflow() {
    #[cfg(target_os = "windows")]
    let shell_command = vec![
        "powershell".to_string(),
        "-Command".to_string(),
        "Start-Sleep -Seconds 10".to_string(),
    ];
    #[cfg(not(target_os = "windows"))]
    let shell_command = vec!["sleep".to_string(), "10".to_string()];

    let codex_home = tempdir().expect("tempdir");
    std::fs::write(codex_home.path().join("tracked.txt"), "review target\n")
        .expect("tracked file should be written");
    let init_output = std::process::Command::new("git")
        .arg("init")
        .current_dir(codex_home.path())
        .output()
        .expect("git init should succeed");
    assert_eq!(init_output.status.success(), true);
    let name_output = std::process::Command::new("git")
        .args(["config", "user.name", "Codex Test"])
        .current_dir(codex_home.path())
        .output()
        .expect("git user.name should be configured");
    assert_eq!(name_output.status.success(), true);
    let email_output = std::process::Command::new("git")
        .args(["config", "user.email", "codex@example.com"])
        .current_dir(codex_home.path())
        .output()
        .expect("git user.email should be configured");
    assert_eq!(email_output.status.success(), true);
    let add_output = std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(codex_home.path())
        .output()
        .expect("git add should succeed");
    assert_eq!(add_output.status.success(), true);
    let commit_output = std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(codex_home.path())
        .output()
        .expect("git commit should succeed");
    assert_eq!(commit_output.status.success(), true);
    let model_server = start_mock_responses_server_sequence(vec![
        create_shell_command_sse_response(
            shell_command,
            Some(codex_home.path()),
            Some(10_000),
            "call-sleep",
        )
        .expect("shell command response should serialize"),
    ])
    .await;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "workspace-write"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            model_server.uri
        ),
    )
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
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
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
                    text: "run sleep".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().to_path_buf()),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();

    timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) = event
                    && notification.thread_id == started.thread.id
                    && notification.turn.id == turn_id
                {
                    break;
                }
            }
        })
        .await
        .expect("turn/started notification should arrive");

    let steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(3),
            params: TurnSteerParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer toward shutdown".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should succeed through embedded gateway");
    assert_eq!(steer_response.turn_id, turn_id);

    let interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(4),
            params: TurnInterruptParams {
                thread_id: started.thread.id.clone(),
                turn_id: turn_id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should succeed through embedded gateway");
    assert_eq!(interrupt_response, TurnInterruptResponse {});

    let mut saw_turn_completed = false;
    timeout(Duration::from_secs(10), async {
        while !saw_turn_completed {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == started.thread.id
                && turn.id == turn_id
                && turn.status == TurnStatus::Interrupted
            {
                saw_turn_completed = true;
            }
        }
    })
    .await
    .expect("turn control notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn embedded_server_preserves_unmaterialized_thread_resume_and_fork_errors_over_v2() {
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

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
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

    let resume_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(2),
            params: ThreadResumeParams {
                thread_id: started.thread.id.clone(),
                history: None,
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect_err("unmaterialized thread/resume should fail through embedded gateway");
    assert_eq!(
        resume_error.to_string(),
        format!(
            "thread/resume failed: no rollout found for thread id {} (code -32600)",
            started.thread.id
        )
    );
    let TypedRequestError::Server {
        source: resume_source,
        ..
    } = resume_error
    else {
        panic!("thread/resume should return a server JSON-RPC error");
    };
    assert_eq!(
        resume_source.message,
        format!("no rollout found for thread id {}", started.thread.id)
    );

    let resume_path_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(3),
            params: ThreadResumeParams {
                thread_id: started.thread.id.clone(),
                history: None,
                path: Some("/tmp/rollout.jsonl".into()),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect_err("unknown path-based thread/resume should fail through embedded gateway");
    assert_eq!(
        resume_path_error.to_string(),
        "thread/resume failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    let fork_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(4),
            params: ThreadForkParams {
                thread_id: started.thread.id.clone(),
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect_err("unmaterialized thread/fork should fail through embedded gateway");
    assert_eq!(
        fork_error.to_string(),
        format!(
            "thread/fork failed: no rollout found for thread id {} (code -32600)",
            started.thread.id
        )
    );
    let TypedRequestError::Server {
        source: fork_source,
        ..
    } = fork_error
    else {
        panic!("thread/fork should return a server JSON-RPC error");
    };
    assert_eq!(
        fork_source.message,
        format!("no rollout found for thread id {}", started.thread.id)
    );

    let fork_path_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: started.thread.id.clone(),
                path: Some("/tmp/rollout.jsonl".into()),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect_err("unknown path-based thread/fork should fail through embedded gateway");
    assert_eq!(
        fork_path_error.to_string(),
        "thread/fork failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

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

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_bootstrap_and_thread_workflow() {
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

    let account: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read should succeed through remote gateway");
    assert_eq!(account.account, None);

    let models: ModelListResponse = client
        .request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(2),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        })
        .await
        .expect("model/list should succeed through remote gateway");
    assert_eq!(models.data.is_empty(), false);

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");
    assert_eq!(started.thread.id, "thread-remote-workflow");

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(4),
            params: ThreadListParams {
                parent_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        })
        .await
        .expect("thread/list should succeed through remote gateway");
    assert_eq!(listed.data.len(), 1);
    assert_eq!(listed.data[0].id, started.thread.id);

    let loaded: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(5),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should succeed through remote gateway");
    assert_eq!(loaded.data, vec![started.thread.id.clone()]);

    let read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed through remote gateway");
    assert_eq!(read.thread.id, started.thread.id);
    assert_eq!(read.thread.name, None);

    let renamed_thread_name = "Remote Gateway Thread".to_string();
    let rename_response: ThreadSetNameResponse = client
        .request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(7),
            params: ThreadSetNameParams {
                thread_id: started.thread.id.clone(),
                name: renamed_thread_name.clone(),
            },
        })
        .await
        .expect("thread/name/set should succeed through remote gateway");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let rename_notification = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
            {
                break notification;
            }
        }
    })
    .await
    .expect("thread/name/updated notification should arrive");
    assert_eq!(
        rename_notification.thread_name,
        Some(renamed_thread_name.clone())
    );

    let renamed: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read after rename should succeed through remote gateway");
    assert_eq!(renamed.thread.name, Some(renamed_thread_name));

    let memory_mode_response: ThreadMemoryModeSetResponse = client
        .request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(9),
            params: ThreadMemoryModeSetParams {
                thread_id: started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("thread/memoryMode/set should succeed through remote gateway");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(10),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello from remote gateway".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
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
        .expect("turn/start should succeed through remote gateway");
    assert_eq!(turn_started_response.turn.id, "turn-remote-workflow");
    assert_eq!(turn_started_response.turn.status, TurnStatus::InProgress);

    let mut coverage = TurnStreamingCoverage::default();
    let mut extended_notifications = HashSet::new();
    let turn_lifecycle_result = timeout(Duration::from_secs(5), async {
        while coverage
            != (TurnStreamingCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_hook_started: true,
                saw_item_started: true,
                saw_agent_delta: true,
                saw_reasoning_summary_delta: true,
                saw_reasoning_text_delta: true,
                saw_command_output_delta: true,
                saw_file_change_delta: true,
                saw_hook_completed: true,
                saw_item_completed: true,
                saw_turn_completed: true,
            })
            || extended_notifications != expected_extended_turn_notifications()
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && matches!(
                        notification.status,
                        ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                    ) =>
                {
                    coverage.saw_thread_active = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == "turn-remote-workflow" =>
                {
                    coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-remote-workflow")
                    && notification.run.id == format!("hook-{}", started.thread.id) =>
                {
                    coverage.saw_hook_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", started.thread.id)
                            && text == "streaming answer in progress"
                    ) =>
                {
                    coverage.saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.delta == "hello back" =>
                {
                    coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification))
                    if notification.thread_id == started.thread.id
                        && notification.turn_id == "turn-remote-workflow"
                        && notification.delta == "remote plan" =>
                {
                    extended_notifications.insert("plan_delta");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.delta == "remote summary"
                    && notification.summary_index == 0 =>
                {
                    coverage.saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.summary_index == 0 =>
                {
                    extended_notifications.insert("reasoning_summary_part_added");
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.delta == "remote reasoning"
                    && notification.content_index == 0 =>
                {
                    coverage.saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TerminalInteraction(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.process_id == "proc-remote-workflow"
                    && notification.stdin == "y\n" =>
                {
                    extended_notifications.insert("terminal_interaction");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.delta == "remote stdout" =>
                {
                    coverage.saw_command_output_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.delta == "remote patch" =>
                {
                    coverage.saw_file_change_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnDiffUpdated(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.diff == "remote diff" =>
                {
                    extended_notifications.insert("turn_diff_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnPlanUpdated(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.explanation.as_deref() == Some("single-worker remote plan")
                    && notification.plan.len() == 1
                    && notification.plan[0].step == "remote plan step" =>
                {
                    extended_notifications.insert("turn_plan_updated");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.token_usage.total.total_tokens == 42 =>
                {
                    extended_notifications.insert("thread_token_usage_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::McpToolCallProgress(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.message == "remote mcp progress" =>
                {
                    extended_notifications.insert("mcp_tool_call_progress");
                }
                AppServerEvent::ServerNotification(ServerNotification::ContextCompacted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow" =>
                {
                    extended_notifications.insert("context_compacted");
                }
                AppServerEvent::ServerNotification(ServerNotification::ModelRerouted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.from_model == "gpt-5"
                    && notification.to_model == "gpt-5-codex" =>
                {
                    extended_notifications.insert("model_rerouted");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::RawResponseItemCompleted(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow" =>
                {
                    extended_notifications.insert("raw_response_item_completed");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewStarted(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.review_id == "guardian-thread-remote-workflow"
                    && notification.target_item_id.as_deref()
                        == Some("cmd-thread-remote-workflow") =>
                {
                    extended_notifications.insert("guardian_review_started");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewCompleted(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && notification.review_id == "guardian-thread-remote-workflow"
                    && notification.target_item_id.as_deref()
                        == Some("cmd-thread-remote-workflow") =>
                {
                    extended_notifications.insert("guardian_review_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::Error(notification))
                    if notification.thread_id == started.thread.id
                        && notification.turn_id == "turn-remote-workflow"
                        && !notification.will_retry
                        && notification.error.message == "remote workflow warning" =>
                {
                    extended_notifications.insert("error");
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-remote-workflow")
                    && notification.run.id == format!("hook-{}", started.thread.id) =>
                {
                    coverage.saw_hook_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == "turn-remote-workflow"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", started.thread.id)
                            && text == "streaming answer completed"
                    ) =>
                {
                    coverage.saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == "turn-remote-workflow"
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    coverage.saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        turn_lifecycle_result.is_ok(),
        true,
        "turn lifecycle notifications should arrive: {coverage:?} extended={extended_notifications:?}"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_v2_thread_reentry_from_later_client_session() {
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

    let owner_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("owner client should connect to remote gateway");

    let started: AppServerThreadStartResponse = owner_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                cwd: Some("/tmp/remote-project".to_string()),
                ephemeral: Some(true),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");
    assert_eq!(started.thread.id, "thread-remote-workflow");

    assert_remote_client_shutdown(owner_client.shutdown().await);

    let reentry_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("reentry client should connect to remote gateway");

    let listed: AppServerThreadListResponse = reentry_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(2),
            params: ThreadListParams {
                parent_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        })
        .await
        .expect("thread/list should succeed for later remote client");
    assert_eq!(listed.data.len(), 1);
    assert_eq!(listed.data[0].id, started.thread.id);

    let read: AppServerThreadReadResponse = reentry_client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(3),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for later remote client");
    assert_eq!(read.thread.id, started.thread.id);

    assert_remote_client_shutdown(reentry_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_realtime_workflow() {
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

    let voices: ThreadRealtimeListVoicesResponse = client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed through remote gateway");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList::builtin(),
        }
    );

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");

    let realtime_started: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("thread/realtime/start should succeed through remote gateway");
    assert_eq!(realtime_started, ThreadRealtimeStartResponse {});

    let append_text: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeAppendTextParams {
                thread_id: started.thread.id.clone(),
                text: "hello realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should succeed through remote gateway");
    assert_eq!(append_text, ThreadRealtimeAppendTextResponse {});

    let append_audio: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-remote-workflow".to_string()),
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should succeed through remote gateway");
    assert_eq!(append_audio, ThreadRealtimeAppendAudioResponse {});

    let stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeStopParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should succeed through remote gateway");
    assert_eq!(stop_response, ThreadRealtimeStopResponse {});

    let mut coverage = RealtimeStreamingCoverage::default();
    timeout(Duration::from_secs(5), async {
        while coverage
            != (RealtimeStreamingCoverage {
                saw_started: true,
                saw_item_added: true,
                saw_output_audio_delta: true,
                saw_transcript_delta: true,
                saw_transcript_done: true,
                saw_sdp: true,
                saw_error: true,
                saw_closed: true,
            })
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.realtime_session_id.as_deref()
                        == Some("session-remote-workflow")
                    && notification.version == RealtimeConversationVersion::V2 =>
                {
                    coverage.saw_started = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.delta == "remote realtime delta" =>
                {
                    coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.text == "remote realtime done" =>
                {
                    coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.audio.data == "AQID"
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1 =>
                {
                    coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.message == "remote realtime warning" =>
                {
                    coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.reason.as_deref() == Some("remote-client-requested") =>
                {
                    coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime workflow notifications should arrive");

    assert_eq!(
        coverage,
        RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_turn_control_workflow() {
    let websocket_url = start_mock_remote_multi_connection_turn_control_server(
        "thread-remote-workflow",
        "/tmp/remote-project",
        "remote steer delta",
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

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");

    let turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start turn".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
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
        .expect("turn/start should succeed through remote gateway");
    assert_eq!(turn_started.turn.id, "turn-thread-remote-workflow");

    let steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(3),
            params: TurnSteerParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "continue".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should succeed through remote gateway");
    assert_eq!(steer_response.turn_id, turn_started.turn.id);

    let interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(4),
            params: TurnInterruptParams {
                thread_id: started.thread.id.clone(),
                turn_id: turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should succeed through remote gateway");
    assert_eq!(interrupt_response, TurnInterruptResponse {});

    let mut coverage = TurnControlCoverage::default();
    timeout(Duration::from_secs(5), async {
        while coverage
            != (TurnControlCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_agent_delta: true,
                saw_turn_completed: true,
                saw_thread_idle: true,
            })
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => match notification.status {
                    ThreadStatus::Active { ref active_flags }
                        if notification.thread_id == started.thread.id
                            && active_flags.is_empty() =>
                    {
                        coverage.saw_thread_active = true;
                    }
                    ThreadStatus::Idle if notification.thread_id == started.thread.id => {
                        coverage.saw_thread_idle = true;
                    }
                    _ => {}
                },
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_started.turn.id =>
                {
                    coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == "remote steer delta" =>
                {
                    coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_started.turn.id
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    coverage.saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("turn control notifications should arrive");

    assert_eq!(
        coverage,
        TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_thread_control_and_review_workflow() {
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
    .expect("remote client should connect to remote gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed through remote gateway");
    assert_eq!(
        unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(3),
            params: ThreadCompactStartParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed through remote gateway");
    assert_eq!(compact, ThreadCompactStartResponse {});

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(4),
            params: ThreadShellCommandParams {
                thread_id: started.thread.id.clone(),
                command: "git status --short".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed through remote gateway");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(5),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed through remote gateway");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(6),
            params: ThreadRollbackParams {
                thread_id: started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed through remote gateway");
    assert_eq!(rollback.thread.id, started.thread.id);
    assert_eq!(rollback.thread.preview, "/tmp/remote-project");
    assert_eq!(rollback.thread.turns, Vec::new());

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(7),
            params: ThreadArchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed through remote gateway");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(8),
            params: ThreadUnarchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed through remote gateway");
    assert_eq!(unarchive.thread.id, started.thread.id);
    assert_eq!(unarchive.thread.preview, "/tmp/remote-project");

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(9),
            params: ThreadMetadataUpdateParams {
                thread_id: started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed through remote gateway");
    assert_eq!(metadata_update.thread.id, started.thread.id);
    assert_eq!(metadata_update.thread.preview, "/tmp/remote-project");

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(10),
            params: ThreadTurnsListParams {
                thread_id: started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed through remote gateway");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, "turn-remote-workflow");
    assert_eq!(turns.next_cursor, None);

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(11),
            params: ThreadIncrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed through remote gateway");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(12),
            params: ThreadDecrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed through remote gateway");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(13),
            params: ThreadInjectItemsParams {
                thread_id: started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed through remote gateway");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(14),
            params: ReviewStartParams {
                thread_id: started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review the current change".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should succeed through remote gateway");
    assert_eq!(review.turn.id, "turn-review-remote-workflow");
    assert_eq!(review.review_thread_id, "thread-review-remote-workflow");

    let review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(15),
            params: ThreadReadParams {
                thread_id: review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for review thread through remote gateway");
    assert_eq!(review_thread.thread.id, review.review_thread_id);
    assert_eq!(review_thread.thread.preview, "/tmp/remote-project/review");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

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

    let detected: ExternalAgentConfigDetectResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigDetectParams {
                include_home: false,
                cwds: Some(vec![PathBuf::from("/tmp/remote-project")]),
            },
        })
        .await
        .expect("externalAgentConfig/detect should succeed through remote gateway");
    assert_eq!(detected.items.is_empty(), true);

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(2),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should succeed through remote gateway");
    assert!(!imported.import_id.is_empty());

    let import_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::ExternalAgentConfigImportCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("externalAgentConfig/import/completed notification should arrive");
    assert_eq!(import_completed.item_type_results, Vec::new());
    assert!(!import_completed.import_id.is_empty());

    let config_read: ConfigReadResponse = client
        .request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(3),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/remote-project".to_string()),
            },
        })
        .await
        .expect("config/read should succeed through remote gateway");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5"));
    assert_eq!(config_read.layers, None);

    let config_requirements: ConfigRequirementsReadResponse = client
        .request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(4),
            params: None,
        })
        .await
        .expect("configRequirements/read should succeed through remote gateway");
    assert_eq!(config_requirements.requirements, None);

    let experimental_features: ExperimentalFeatureListResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(5),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(20),
                ..Default::default()
            },
        })
        .await
        .expect("experimentalFeature/list should succeed through remote gateway");
    assert_eq!(experimental_features.data.len(), 1);
    assert_eq!(experimental_features.data[0].name, "gateway-test-feature");

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(41),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        })
        .await
        .expect("experimentalFeature/enablement/set should succeed through remote gateway");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let collaboration_modes: CollaborationModeListResponse = client
        .request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(6),
            params: CollaborationModeListParams::default(),
        })
        .await
        .expect("collaborationMode/list should succeed through remote gateway");
    assert_eq!(collaboration_modes.data.len(), 1);
    assert_eq!(collaboration_modes.data[0].name, "default");

    let rate_limits: GetAccountRateLimitsResponse = client
        .request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(7),
            params: None,
        })
        .await
        .expect("account/rateLimits/read should succeed through remote gateway");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("codex"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Codex")
    );

    let legacy_auth: GetAuthStatusResponse = client
        .request_typed(ClientRequest::GetAuthStatus {
            request_id: RequestId::Integer(49),
            params: GetAuthStatusParams {
                include_token: Some(true),
                refresh_token: Some(false),
            },
        })
        .await
        .expect("getAuthStatus should succeed through remote gateway");
    assert_eq!(
        legacy_auth,
        GetAuthStatusResponse {
            auth_method: None,
            auth_token: Some("remote-auth-token".to_string()),
            requires_openai_auth: Some(false),
        }
    );

    let skills: SkillsListResponse = client
        .request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(8),
            params: SkillsListParams {
                cwds: vec![PathBuf::from("/tmp/remote-project")],
                force_reload: false,
            },
        })
        .await
        .expect("skills/list should succeed through remote gateway");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(skills.data[0].cwd, PathBuf::from("/tmp/remote-project"));
    assert_eq!(skills.data[0].errors, Vec::new());
    assert_eq!(skills.data[0].skills, Vec::new());

    let skills_config: SkillsConfigWriteResponse = client
        .request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(42),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/remote-project/skills/remote-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        })
        .await
        .expect("skills/config/write should succeed through remote gateway");
    assert_eq!(skills_config.effective_enabled, true);

    let apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(9),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        })
        .await
        .expect("app/list should succeed through remote gateway");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "remote-app");
    assert_eq!(apps.data[0].name, "Remote App");

    let mcp_statuses: ListMcpServerStatusResponse = client
        .request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(10),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: Some(10),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        })
        .await
        .expect("mcpServerStatus/list should succeed through remote gateway");
    assert_eq!(mcp_statuses.next_cursor, None);
    assert_eq!(mcp_statuses.data.len(), 1);
    assert_eq!(mcp_statuses.data[0].name, "remote-mcp");

    let mcp_refresh: McpServerRefreshResponse = client
        .request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(43),
            params: None,
        })
        .await
        .expect("config/mcpServer/reload should succeed through remote gateway");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let mcp_oauth_login: McpServerOauthLoginResponse = client
        .request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(11),
            params: McpServerOauthLoginParams {
                name: "remote-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
            },
        })
        .await
        .expect("mcpServer/oauth/login should succeed through remote gateway");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/remote-mcp"
    );

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
    assert_eq!(mcp_oauth_completed.name, "remote-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

    let started_for_mcp: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(48),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
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
        .expect("thread/start should make MCP thread-scoped requests visible");
    assert_eq!(started_for_mcp.thread.id, "thread-remote-workflow");

    let mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(44),
            params: McpResourceReadParams {
                thread_id: Some(started_for_mcp.thread.id.clone()),
                server: "remote-mcp".to_string(),
                uri: "file:///tmp/remote-project/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should succeed through remote gateway");
    assert_eq!(mcp_resource.contents.len(), 1);

    let mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(45),
            params: McpServerToolCallParams {
                thread_id: started_for_mcp.thread.id.clone(),
                server: "remote-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({
                    "query": "gateway",
                })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should succeed through remote gateway");
    assert_eq!(mcp_tool.content.len(), 1);
    assert_eq!(mcp_tool.is_error, Some(false));

    let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
        "cwds": ["/tmp/remote-project"],
    }))
    .expect("plugin/list params should deserialize");
    let plugins: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(12),
            params: plugin_list_params,
        })
        .await
        .expect("plugin/list should succeed through remote gateway");
    assert_eq!(plugins.marketplaces.len(), 1);
    assert_eq!(plugins.marketplaces[0].name, "remote-marketplace");
    assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
    assert_eq!(plugins.marketplaces[0].plugins[0].name, "remote-plugin");
    assert_eq!(plugins.marketplaces[0].plugins[0].installed, false);

    let marketplace: MarketplaceAddResponse = client
        .request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(46),
            params: MarketplaceAddParams {
                source: "https://example.com/remote-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/remote-plugin".to_string()]),
            },
        })
        .await
        .expect("marketplace/add should succeed through remote gateway");
    assert_eq!(marketplace.marketplace_name, "remote-marketplace");
    assert_eq!(marketplace.already_added, false);
    assert_eq!(
        marketplace.installed_root.as_path(),
        PathBuf::from("/tmp/remote-project/marketplace").as_path()
    );

    let plugin_read_params: PluginReadParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": "/tmp/remote-project/marketplace.json",
        "pluginName": "remote-plugin",
    }))
    .expect("plugin/read params should deserialize");
    let plugin: PluginReadResponse = client
        .request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(13),
            params: plugin_read_params,
        })
        .await
        .expect("plugin/read should succeed through remote gateway");
    assert_eq!(plugin.plugin.summary.name, "remote-plugin");
    assert_eq!(plugin.plugin.marketplace_name, "remote-marketplace");
    assert_eq!(plugin.plugin.skills.len(), 1);
    assert_eq!(plugin.plugin.skills[0].name, "remote-skill");

    let plugin_install_params: PluginInstallParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": "/tmp/remote-project/marketplace.json",
        "pluginName": "remote-plugin",
    }))
    .expect("plugin/install params should deserialize");
    let install: PluginInstallResponse = client
        .request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(14),
            params: plugin_install_params,
        })
        .await
        .expect("plugin/install should succeed through remote gateway");
    assert_eq!(
        install,
        PluginInstallResponse {
            auth_policy: PluginAuthPolicy::OnInstall,
            apps_needing_auth: Vec::new(),
        }
    );

    let plugins_after_install: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(15),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/remote-project"],
            }))
            .expect("plugin/list params after install should deserialize"),
        })
        .await
        .expect("plugin/list after install should succeed through remote gateway");
    assert_eq!(
        plugins_after_install.marketplaces[0].plugins[0].installed,
        true
    );

    let uninstall: PluginUninstallResponse = client
        .request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(16),
            params: PluginUninstallParams {
                plugin_id: "remote-plugin@remote-marketplace".to_string(),
            },
        })
        .await
        .expect("plugin/uninstall should succeed through remote gateway");
    assert_eq!(uninstall, PluginUninstallResponse {});

    let plugins_after_uninstall: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(17),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/remote-project"],
            }))
            .expect("plugin/list params after uninstall should deserialize"),
        })
        .await
        .expect("plugin/list after uninstall should succeed through remote gateway");
    assert_eq!(
        plugins_after_uninstall.marketplaces[0].plugins[0].installed,
        false
    );

    let batch_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(18),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/remote-project/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite should succeed through remote gateway");
    assert_eq!(batch_write.status, WriteStatus::Ok);
    assert_eq!(batch_write.version, "remote-version-1");
    assert_eq!(
        batch_write.file_path.as_path(),
        PathBuf::from("/tmp/remote-project/config.toml").as_path()
    );
    assert_eq!(batch_write.overridden_metadata, None);

    let config_value_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(19),
            params: ConfigValueWriteParams {
                key_path: "plugins.remote-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        })
        .await
        .expect("config/value/write should succeed through remote gateway");
    assert_eq!(config_value_write.status, WriteStatus::Ok);
    assert_eq!(config_value_write.version, "remote-version-1");
    assert_eq!(
        config_value_write.file_path.as_path(),
        PathBuf::from("/tmp/remote-project/config.toml").as_path()
    );
    assert_eq!(config_value_write.overridden_metadata, None);

    let watch: FsWatchResponse = client
        .request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(20),
            params: FsWatchParams {
                watch_id: "remote-watch".to_string(),
                path: PathBuf::from("/tmp/remote-project/config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        })
        .await
        .expect("fs/watch should succeed through remote gateway");
    assert_eq!(
        watch.path.as_ref(),
        PathBuf::from("/tmp/remote-project/config.toml").as_path()
    );

    let fs_changed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed notification should arrive after fs/watch");
    assert_eq!(fs_changed.watch_id, "remote-watch");
    assert_eq!(
        fs_changed
            .changed_paths
            .iter()
            .map(|path| path.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec!["/tmp/remote-project/config.toml".to_string()]
    );

    let unwatch: FsUnwatchResponse = client
        .request_typed(ClientRequest::FsUnwatch {
            request_id: RequestId::Integer(21),
            params: FsUnwatchParams {
                watch_id: "remote-watch".to_string(),
            },
        })
        .await
        .expect("fs/unwatch should succeed through remote gateway");
    assert_eq!(unwatch, FsUnwatchResponse {});

    let create_directory: FsCreateDirectoryResponse = client
        .request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(33),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/remote-project/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        })
        .await
        .expect("fs/createDirectory should succeed through remote gateway");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = client
        .request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(34),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVtb3RlLWZpbGU=".to_string(),
            },
        })
        .await
        .expect("fs/writeFile should succeed through remote gateway");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = client
        .request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(35),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        })
        .await
        .expect("fs/readFile should succeed through remote gateway");
    assert_eq!(read_file.data_base64, "cmVtb3RlLWZpbGU=");

    let metadata: FsGetMetadataResponse = client
        .request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(36),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        })
        .await
        .expect("fs/getMetadata should succeed through remote gateway");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = client
        .request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(37),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/remote-project/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        })
        .await
        .expect("fs/readDirectory should succeed through remote gateway");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");
    assert_eq!(directory.entries[0].is_file, true);

    let copy: FsCopyResponse = client
        .request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(38),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/remote-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/remote-project/nested/gateway-copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        })
        .await
        .expect("fs/copy should succeed through remote gateway");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = client
        .request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(39),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/remote-project/nested/gateway-copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(true),
                force: Some(true),
            },
        })
        .await
        .expect("fs/remove should succeed through remote gateway");
    assert_eq!(remove, FsRemoveResponse {});

    let fuzzy_search: FuzzyFileSearchResponse = client
        .request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(29),
            params: FuzzyFileSearchParams {
                query: "gate".to_string(),
                roots: vec!["/tmp/remote-project".to_string()],
                cancellation_token: Some("remote-fuzzy-search".to_string()),
            },
        })
        .await
        .expect("fuzzyFileSearch should succeed through remote gateway");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].root, "/tmp/remote-project");
    assert_eq!(fuzzy_search.files[0].path, "docs/gateway.md");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let diff: GitDiffToRemoteResponse = client
        .request_typed(ClientRequest::GitDiffToRemote {
            request_id: RequestId::Integer(50),
            params: GitDiffToRemoteParams {
                cwd: PathBuf::from("/tmp/remote-project"),
            },
        })
        .await
        .expect("gitDiffToRemote should succeed through remote gateway");
    assert_eq!(diff.sha.0, "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(
        diff.diff,
        "diff --git a/docs/gateway.md b/docs/gateway.md\n"
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(30),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "remote-fuzzy-session".to_string(),
                roots: vec!["/tmp/remote-project".to_string()],
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStart should succeed through remote gateway");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(31),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "remote-fuzzy-session".to_string(),
                query: "gate".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionUpdate should succeed through remote gateway");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "remote-fuzzy-session"
                && notification.query == "gate"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated notification should arrive");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(fuzzy_session_updated.files[0].root, "/tmp/remote-project");
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/gateway.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(32),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "remote-fuzzy-session".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStop should succeed through remote gateway");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "remote-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted notification should arrive");
    assert_eq!(fuzzy_session_completed.session_id, "remote-fuzzy-session");

    let windows_setup: WindowsSandboxSetupStartResponse = client
        .request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(40),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/remote-project")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        })
        .await
        .expect("windowsSandbox/setupStart should succeed through remote gateway");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted notification should arrive");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);

    let canceled_login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(22),
            params: LoginAccountParams::ChatgptDeviceCode,
        })
        .await
        .expect("account/login/start should succeed through remote gateway");
    let canceled_login_id = match canceled_login {
        LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            verification_url,
            user_code,
        } => {
            assert_eq!(verification_url, "https://example.com/device");
            assert_eq!(user_code, "REMOTE-CODE");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let cancel_login: CancelLoginAccountResponse = client
        .request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(23),
            params: CancelLoginAccountParams {
                login_id: canceled_login_id,
            },
        })
        .await
        .expect("account/login/cancel should succeed through remote gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::Canceled);

    let completed_login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(24),
            params: LoginAccountParams::Chatgpt {
                codex_streamlined_login: false,
            },
        })
        .await
        .expect("second account/login/start should succeed through remote gateway");
    let completed_login_id = match completed_login {
        LoginAccountResponse::Chatgpt { login_id, auth_url } => {
            assert_eq!(auth_url, "https://example.com/login");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

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
    assert_eq!(login_completed.login_id, Some(completed_login_id));
    assert_eq!(login_completed.success, true);
    assert_eq!(login_completed.error, None);

    let add_credits: SendAddCreditsNudgeEmailResponse = client
        .request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(47),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        })
        .await
        .expect("account/sendAddCreditsNudgeEmail should succeed through remote gateway");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let feedback: FeedbackUploadResponse = client
        .request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(25),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway parity regression".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        })
        .await
        .expect("feedback/upload should succeed through remote gateway");
    assert_eq!(feedback.thread_id, "feedback-thread-remote");

    let command_exec: CommandExecResponse = client
        .request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(26),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf remote-command".to_string(),
                ],
                process_id: Some("proc-remote".to_string()),
                tty: true,
                stream_stdin: true,
                stream_stdout_stderr: true,
                output_bytes_cap: None,
                disable_output_cap: false,
                disable_timeout: false,
                timeout_ms: None,
                cwd: None,
                env: None,
                size: Some(CommandExecTerminalSize { rows: 24, cols: 80 }),
                sandbox_policy: None,
                permission_profile: None,
            },
        })
        .await
        .expect("command/exec should succeed through remote gateway");
    assert_eq!(
        command_exec,
        CommandExecResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    );

    let command_output = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta notification should arrive through remote gateway");
    assert_eq!(command_output.process_id, "proc-remote");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "cmVtb3RlLWNvbW1hbmQ=");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = client
        .request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(26),
            params: CommandExecWriteParams {
                process_id: "proc-remote".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        })
        .await
        .expect("command/exec/write should succeed through remote gateway");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = client
        .request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(27),
            params: CommandExecResizeParams {
                process_id: "proc-remote".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        })
        .await
        .expect("command/exec/resize should succeed through remote gateway");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = client
        .request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(28),
            params: CommandExecTerminateParams {
                process_id: "proc-remote".to_string(),
            },
        })
        .await
        .expect("command/exec/terminate should succeed through remote gateway");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    let reset: MemoryResetResponse = client
        .request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(27),
            params: None,
        })
        .await
        .expect("memory/reset should succeed through remote gateway");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = client
        .request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(28),
            params: None,
        })
        .await
        .expect("account/logout should succeed through remote gateway");
    assert_eq!(logout, LogoutAccountResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}


#[path = "embedded_tests_remote_late.rs"]
mod embedded_tests_remote_late;
