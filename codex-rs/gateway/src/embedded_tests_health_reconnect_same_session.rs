use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_after_disconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_thread_server_for_same_session_recovery(
        "thread-worker-a-3",
        "/tmp/worker-a-3",
    )
    .await;
    let worker_b =
        start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b").await;
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before same-session recovery test starts");

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
        .expect("same northbound session should reuse recovered worker for thread/start");

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
        .expect("shared session should still round-robin onto surviving worker");
    let recovered_started = match (
        first_started.thread.id.as_str(),
        second_started.thread.id.as_str(),
    ) {
        ("thread-worker-a-3", "thread-worker-b") => &first_started,
        ("thread-worker-b", "thread-worker-a-3") => &second_started,
        (first, second) => panic!("unexpected thread ids after reconnect: {first}, {second}"),
    };
    let recovered_thread_id = recovered_started.thread.id.clone();

    let first_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(3),
            params: ThreadReadParams {
                thread_id: recovered_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to the re-added worker");
    assert_eq!(first_read.thread.id, recovered_thread_id);
    assert_eq!(first_read.thread.preview, "/tmp/worker-a-3");

    let resumed: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: recovered_thread_id.clone(),
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
        .expect("thread/resume should route to the re-added worker");
    assert_eq!(resumed.thread.id, recovered_thread_id);
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a-3");

    let rollout_path = recovered_started
        .thread
        .path
        .clone()
        .expect("recovered thread should include a rollout path");
    let resumed_from_path: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(104),
            params: ThreadResumeParams {
                thread_id: recovered_thread_id.clone(),
                history: None,
                path: Some(rollout_path.clone()),
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
        .expect("path-based thread/resume should route to the re-added worker");
    assert_eq!(resumed_from_path.thread.id, recovered_thread_id);
    assert_eq!(resumed_from_path.thread.path, Some(rollout_path.clone()));

    let forked: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: recovered_thread_id.clone(),
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
        .expect("thread/fork should route to the re-added worker");
    assert_eq!(forked.thread.id, "thread-worker-a-3-fork");
    assert_eq!(
        forked.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-3-fork"
    );
    let forked_from_path: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(105),
            params: ThreadForkParams {
                thread_id: recovered_thread_id.clone(),
                path: Some(rollout_path),
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
        .expect("path-based thread/fork should route to the re-added worker");
    assert_eq!(
        forked_from_path.thread.forked_from_id,
        Some(recovered_thread_id.clone())
    );
    assert_eq!(forked_from_path.thread.path.is_some(), true);

    let forked_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: forked.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("forked thread read should route to the re-added worker");
    assert_eq!(forked_read.thread.id, forked.thread.id);
    assert_eq!(forked_read.thread.preview, "/tmp/worker-a-3-fork");

    let first_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(7),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(recovered_thread_id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to the re-added worker");
    assert_eq!(first_apps.next_cursor, None);
    assert_eq!(
        first_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-3-app"]
    );

    let mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(107),
            params: McpResourceReadParams {
                thread_id: Some(recovered_thread_id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-a-3/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to the re-added worker");
    assert_eq!(
        serde_json::to_value(&mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-a-3/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-a-3 recovered resource",
        }])
    );

    let mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(108),
            params: McpServerToolCallParams {
                thread_id: recovered_thread_id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-a-3" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to the re-added worker");
    assert_eq!(
        mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-a-3" }))
    );
    assert_eq!(mcp_tool.is_error, Some(false));

    let renamed_thread_name = "Recovered Worker Thread".to_string();
    let rename_response: ThreadSetNameResponse = client
        .request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(8),
            params: ThreadSetNameParams {
                thread_id: recovered_thread_id.clone(),
                name: renamed_thread_name.clone(),
            },
        })
        .await
        .expect("thread/name/set should route to the re-added worker");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let renamed_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(9),
            params: ThreadReadParams {
                thread_id: recovered_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read after rename should still route to the re-added worker");
    assert_eq!(renamed_read.thread.id, "thread-worker-a-3");
    assert_eq!(renamed_read.thread.preview, "/tmp/worker-a-3");
    assert_eq!(renamed_read.thread.name, Some(renamed_thread_name));

    let rename_notification = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && notification.thread_id == recovered_thread_id
            {
                break notification;
            }
        }
    })
    .await
    .expect("thread/name/updated should fan in after same-session recovery");
    assert_eq!(rename_notification.thread_name, renamed_read.thread.name);

    let memory_mode_response: ThreadMemoryModeSetResponse = client
        .request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(10),
            params: ThreadMemoryModeSetParams {
                thread_id: first_started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("thread/memoryMode/set should route to the re-added worker");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    let unsubscribe_response: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(11),
            params: ThreadUnsubscribeParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should route to the re-added worker");
    assert_eq!(
        unsubscribe_response,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let archive_response: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(12),
            params: ThreadArchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should route to the re-added worker");
    assert_eq!(archive_response, ThreadArchiveResponse {});

    let unarchive_response: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(13),
            params: ThreadUnarchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should route to the re-added worker");
    assert_eq!(unarchive_response.thread.id, first_started.thread.id);
    assert_eq!(unarchive_response.thread.preview, "/tmp/worker-a-3");

    let expected_thread_lifecycle_notifications = HashSet::from([
        (first_started.thread.id.clone(), "closed"),
        (first_started.thread.id.clone(), "archived"),
        (first_started.thread.id.clone(), "unarchived"),
    ]);
    let mut thread_lifecycle_notifications = HashSet::new();
    let lifecycle_result = timeout(Duration::from_secs(5), async {
        while thread_lifecycle_notifications != expected_thread_lifecycle_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "closed"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadArchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "archived"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadUnarchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "unarchived"));
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        lifecycle_result.is_ok(),
        "thread lifecycle notifications should fan in after same-session recovery: {thread_lifecycle_notifications:?}"
    );

    let metadata_update_response: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(14),
            params: ThreadMetadataUpdateParams {
                thread_id: first_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-recovered-worker".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("thread/metadata/update should route to the re-added worker");
    assert_eq!(metadata_update_response.thread.id, first_started.thread.id);
    assert_eq!(metadata_update_response.thread.preview, "/tmp/worker-a-3");

    let turns_response: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(15),
            params: ThreadTurnsListParams {
                thread_id: first_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should route to the re-added worker");
    assert_eq!(turns_response.data.len(), 1);
    assert_eq!(turns_response.data[0].id, "turn-thread-worker-a-3");
    assert_eq!(turns_response.next_cursor, None);
    assert_eq!(turns_response.backwards_cursor, None);

    let increment_response: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadIncrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should route to the re-added worker");
    assert_eq!(
        increment_response,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_response: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(17),
            params: ThreadDecrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should route to the re-added worker");
    assert_eq!(
        decrement_response,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_response: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(18),
            params: ThreadInjectItemsParams {
                thread_id: first_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Recovered worker inject"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should route to the re-added worker");
    assert_eq!(inject_response, ThreadInjectItemsResponse {});

    let compact_response: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should route to the re-added worker");
    assert_eq!(compact_response, ThreadCompactStartResponse {});

    let shell_command_response: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(20),
            params: ThreadShellCommandParams {
                thread_id: first_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should route to the re-added worker");
    assert_eq!(shell_command_response, ThreadShellCommandResponse {});

    let background_clean_response: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(21),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should route to the re-added worker");
    assert_eq!(
        background_clean_response,
        ThreadBackgroundTerminalsCleanResponse {}
    );

    let rollback_response: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(22),
            params: ThreadRollbackParams {
                thread_id: first_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should route to the re-added worker");
    assert_eq!(rollback_response.thread.id, first_started.thread.id);
    assert_eq!(rollback_response.thread.preview, "/tmp/worker-a-3");

    let review_response: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(23),
            params: ReviewStartParams {
                thread_id: first_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review recovered worker".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should route to the re-added worker");
    assert_eq!(review_response.turn.id, "turn-review-thread-worker-a-3");
    assert_eq!(review_response.review_thread_id, "thread-worker-a-3-review");

    let review_thread_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(24),
            params: ThreadReadParams {
                thread_id: review_response.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("review thread read should route to the re-added worker");
    assert_eq!(
        review_thread_read.thread.id,
        review_response.review_thread_id
    );
    assert_eq!(review_thread_read.thread.preview, "/tmp/worker-a-3");

    let turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(25),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Recovered worker turn".to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should route to the re-added worker");
    assert_eq!(turn_started.turn.id, "turn-thread-worker-a-3");

    let mut recovered_turn_coverage = TurnStreamingCoverage::default();
    let mut recovered_extended_turn_notifications = HashSet::new();
    let recovered_turn_result = timeout(Duration::from_secs(5), async {
        while recovered_turn_coverage
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
            || recovered_extended_turn_notifications != expected_extended_turn_notifications()
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && matches!(
                        notification.status,
                        ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                    ) =>
                {
                    recovered_turn_coverage.saw_thread_active = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == turn_started.turn.id =>
                {
                    recovered_turn_coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some(turn_started.turn.id.as_str())
                    && notification.run.id == format!("hook-{}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_hook_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", first_started.thread.id)
                            && text == "streaming answer in progress"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == "hello from recovered worker" =>
                {
                    recovered_turn_coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification))
                    if notification.thread_id == first_started.thread.id
                        && notification.turn_id == turn_started.turn.id
                        && notification.delta == format!("plan {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("plan_delta");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("summary {}", first_started.thread.id)
                    && notification.summary_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.summary_index == 0 =>
                {
                    recovered_extended_turn_notifications.insert("reasoning_summary_part_added");
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("reasoning {}", first_started.thread.id)
                    && notification.content_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TerminalInteraction(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.process_id == format!("proc-{}", first_started.thread.id)
                    && notification.stdin == "y\n" =>
                {
                    recovered_extended_turn_notifications.insert("terminal_interaction");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("stdout {}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_command_output_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("patch {}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_file_change_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnDiffUpdated(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.diff == format!("diff {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("turn_diff_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnPlanUpdated(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.explanation.as_deref() == Some("gateway multi-worker plan")
                    && notification.plan.len() == 1
                    && notification.plan[0].step == format!("plan {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("turn_plan_updated");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.token_usage.total.total_tokens == 42 =>
                {
                    recovered_extended_turn_notifications.insert("thread_token_usage_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::McpToolCallProgress(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.message
                        == format!("mcp progress {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("mcp_tool_call_progress");
                }
                AppServerEvent::ServerNotification(ServerNotification::ContextCompacted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id =>
                {
                    recovered_extended_turn_notifications.insert("context_compacted");
                }
                AppServerEvent::ServerNotification(ServerNotification::ModelRerouted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.from_model == "gpt-5"
                    && notification.to_model == "gpt-5-codex" =>
                {
                    recovered_extended_turn_notifications.insert("model_rerouted");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::RawResponseItemCompleted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id =>
                {
                    recovered_extended_turn_notifications.insert("raw_response_item_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::Error(notification))
                    if notification.thread_id == first_started.thread.id
                        && notification.turn_id == turn_started.turn.id
                        && !notification.will_retry
                        && notification.error.message
                            == format!("recoverable warning {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("error");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewStarted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.review_id
                        == format!("guardian-{}", first_started.thread.id)
                    && notification.target_item_id.as_deref()
                        == Some(&format!("cmd-{}", first_started.thread.id)) =>
                {
                    recovered_extended_turn_notifications.insert("guardian_review_started");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewCompleted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.review_id
                        == format!("guardian-{}", first_started.thread.id)
                    && notification.target_item_id.as_deref()
                        == Some(&format!("cmd-{}", first_started.thread.id)) =>
                {
                    recovered_extended_turn_notifications.insert("guardian_review_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some(turn_started.turn.id.as_str())
                    && notification.run.id == format!("hook-{}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_hook_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", first_started.thread.id)
                            && text == "streaming answer completed"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == turn_started.turn.id =>
                {
                    recovered_turn_coverage.saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        recovered_turn_result.is_ok(),
        true,
        "turn notifications should fan in after same-session recovery: lifecycle={recovered_turn_coverage:?} extended={recovered_extended_turn_notifications:?}"
    );

    let turn_steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(26),
            params: TurnSteerParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Steer recovered worker".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should route to the re-added worker");
    assert_eq!(turn_steer_response.turn_id, turn_started.turn.id);

    let turn_interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(27),
            params: TurnInterruptParams {
                thread_id: first_started.thread.id.clone(),
                turn_id: turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should route to the re-added worker");
    assert_eq!(turn_interrupt_response, TurnInterruptResponse {});

    let realtime_start_response: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(28),
            params: ThreadRealtimeStartParams {
                thread_id: first_started.thread.id.clone(),
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
        .expect("thread/realtime/start should route to the re-added worker");
    assert_eq!(realtime_start_response, ThreadRealtimeStartResponse {});

    let realtime_append_text_response: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(29),
            params: ThreadRealtimeAppendTextParams {
                thread_id: first_started.thread.id.clone(),
                text: "Recovered worker realtime text".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should route to the re-added worker");
    assert_eq!(
        realtime_append_text_response,
        ThreadRealtimeAppendTextResponse {}
    );

    let realtime_append_audio_response: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(30),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: first_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("recovered-worker-audio".to_string()),
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should route to the re-added worker");
    assert_eq!(
        realtime_append_audio_response,
        ThreadRealtimeAppendAudioResponse {}
    );

    let realtime_stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(31),
            params: ThreadRealtimeStopParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should route to the re-added worker");
    assert_eq!(realtime_stop_response, ThreadRealtimeStopResponse {});

    let mut recovered_realtime_coverage = RealtimeStreamingCoverage::default();
    timeout(Duration::from_secs(5), async {
        while recovered_realtime_coverage
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
                .expect("event stream should stay open after same-session recovery");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.realtime_session_id.as_deref()
                        == Some(format!("session-{}", first_started.thread.id).as_str())
                    && notification.version == RealtimeConversationVersion::V2 =>
                {
                    recovered_realtime_coverage.saw_started = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    recovered_realtime_coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1
                    && notification.audio.data == "AQID" =>
                {
                    recovered_realtime_coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.delta == format!("delta {}", first_started.thread.id)
                    && notification.role == "assistant" =>
                {
                    recovered_realtime_coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.text == format!("done {}", first_started.thread.id)
                    && notification.role == "assistant" =>
                {
                    recovered_realtime_coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    recovered_realtime_coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.message == "realtime transport warning" =>
                {
                    recovered_realtime_coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.reason.as_deref() == Some("client requested stop") =>
                {
                    recovered_realtime_coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime notifications should fan in after same-session recovery");

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(32),
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
        .expect("thread/list should include recovered and surviving workers");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a-3"]
    );

    let loaded_listed: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(33),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should include recovered and surviving workers");
    assert_eq!(loaded_listed.next_cursor, None);
    assert_eq!(
        loaded_listed
            .data
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-3", "thread-worker-b"]
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
