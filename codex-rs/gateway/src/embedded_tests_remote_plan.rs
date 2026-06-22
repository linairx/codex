use super::*;

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
