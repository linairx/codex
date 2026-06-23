use super::*;
use pretty_assertions::assert_eq;

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
