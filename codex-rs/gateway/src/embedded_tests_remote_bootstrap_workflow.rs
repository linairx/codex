use super::*;
use pretty_assertions::assert_eq;

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
                ancestor_thread_id: None,
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
