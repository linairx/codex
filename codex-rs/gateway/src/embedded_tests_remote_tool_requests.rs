use super::*;
use pretty_assertions::assert_eq;

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
