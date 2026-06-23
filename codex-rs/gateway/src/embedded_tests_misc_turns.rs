use super::*;
use pretty_assertions::assert_eq;

fn expected_turn_streaming_coverage() -> TurnStreamingCoverage {
    TurnStreamingCoverage {
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
    }
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_turn_routing_and_notification_fan_in() {
    let worker_a = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "turn-thread-worker-a",
        "hello from worker a",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "turn-thread-worker-b",
        "hello from worker b",
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
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

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
    .expect("remote client should connect to multi-worker gateway");
    let mut client = client;

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
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
        .expect("first thread/start should succeed through multi-worker remote gateway");
    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
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
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello worker a".to_string(),
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
        .expect("first turn/start should route to worker A");
    assert_eq!(first_turn_started.turn.id, "turn-thread-worker-a");
    assert_eq!(first_turn_started.turn.status, TurnStatus::InProgress);

    let second_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello worker b".to_string(),
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
        .expect("second turn/start should route to worker B");
    assert_eq!(second_turn_started.turn.id, "turn-thread-worker-b");
    assert_eq!(second_turn_started.turn.status, TurnStatus::InProgress);

    let mut lifecycle_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            TurnStreamingCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            TurnStreamingCoverage::default(),
        ),
    ]);
    let expected_turn_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            ("turn-thread-worker-a", "hello from worker a"),
        ),
        (
            second_started.thread.id.clone(),
            ("turn-thread-worker-b", "hello from worker b"),
        ),
    ]);
    let mut extended_notifications_by_thread: HashMap<String, HashSet<&'static str>> =
        HashMap::from([
            (first_started.thread.id.clone(), HashSet::new()),
            (second_started.thread.id.clone(), HashSet::new()),
        ]);

    let turn_fan_in_result = timeout(Duration::from_secs(5), async {
        while lifecycle_by_thread
            .values()
            .any(|coverage| *coverage != expected_turn_streaming_coverage())
            || extended_notifications_by_thread
                .values()
                .any(|notifications| *notifications != expected_extended_turn_notifications())
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => {
                    if let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && matches!(
                            notification.status,
                            ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                        )
                    {
                        coverage.saw_thread_active = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                    {
                        coverage.saw_turn_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id.as_deref() == Some(*expected_turn_id)
                        && notification.run.id == format!("hook-{}", notification.thread_id)
                    {
                        coverage.saw_hook_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && matches!(
                            &notification.item,
                            ThreadItem::AgentMessage {
                                id,
                                text,
                                ..
                            } if id == &format!("msg-{}", notification.thread_id)
                                && text == "streaming answer in progress"
                        )
                    {
                        coverage.saw_item_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, expected_delta)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_agent_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification)) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("plan {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("plan_delta");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("summary {}", notification.thread_id)
                        && notification.summary_index == 0
                    {
                        coverage.saw_reasoning_summary_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.summary_index == 0
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("reasoning_summary_part_added");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("reasoning {}", notification.thread_id)
                        && notification.content_index == 0
                    {
                        coverage.saw_reasoning_text_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TerminalInteraction(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.process_id == format!("proc-{}", notification.thread_id)
                        && notification.stdin == "y\n"
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("terminal_interaction");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("stdout {}", notification.thread_id)
                    {
                        coverage.saw_command_output_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == format!("patch {}", notification.thread_id)
                    {
                        coverage.saw_file_change_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnDiffUpdated(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.diff == format!("diff {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("turn_diff_updated");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnPlanUpdated(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.explanation.as_deref() == Some("gateway multi-worker plan")
                        && notification.plan.len() == 1
                        && notification.plan[0].step == format!("plan {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("turn_plan_updated");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.token_usage.total.total_tokens == 42
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("thread_token_usage_updated");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::McpToolCallProgress(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.message
                            == format!("mcp progress {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("mcp_tool_call_progress");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ContextCompacted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("context_compacted");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ModelRerouted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.from_model == "gpt-5"
                        && notification.to_model == "gpt-5-codex"
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("model_rerouted");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::RawResponseItemCompleted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("raw_response_item_completed");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::Error(notification)) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && !notification.will_retry
                        && notification.error.message
                            == format!("recoverable warning {}", notification.thread_id)
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("error");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewStarted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.review_id == format!("guardian-{}", notification.thread_id)
                        && notification.target_item_id.as_deref()
                            == Some(&format!("cmd-{}", notification.thread_id))
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("guardian_review_started");
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewCompleted(notification),
                ) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.review_id == format!("guardian-{}", notification.thread_id)
                        && notification.target_item_id.as_deref()
                            == Some(&format!("cmd-{}", notification.thread_id))
                        && let Some(notifications) =
                            extended_notifications_by_thread.get_mut(&notification.thread_id)
                    {
                        notifications.insert("guardian_review_completed");
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id.as_deref() == Some(*expected_turn_id)
                        && notification.run.id == format!("hook-{}", notification.thread_id)
                    {
                        coverage.saw_hook_completed = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && matches!(
                            &notification.item,
                            ThreadItem::AgentMessage {
                                id,
                                text,
                                ..
                            } if id == &format!("msg-{}", notification.thread_id)
                                && text == "streaming answer completed"
                        )
                    {
                        coverage.saw_item_completed = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                        && notification.turn.status == TurnStatus::Completed
                    {
                        coverage.saw_turn_completed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        turn_fan_in_result.is_ok(),
        true,
        "turn notifications should fan in from both workers: lifecycle={lifecycle_by_thread:?} extended={extended_notifications_by_thread:?}"
    );

    assert_eq!(
        lifecycle_by_thread.get(&first_started.thread.id),
        Some(&expected_turn_streaming_coverage())
    );
    assert_eq!(
        lifecycle_by_thread.get(&second_started.thread.id),
        Some(&expected_turn_streaming_coverage())
    );
    assert_eq!(
        extended_notifications_by_thread.get(&first_started.thread.id),
        Some(&expected_extended_turn_notifications())
    );
    assert_eq!(
        extended_notifications_by_thread.get(&second_started.thread.id),
        Some(&expected_extended_turn_notifications())
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
