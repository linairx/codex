use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_clients_recover_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_thread_server(
        "thread-worker-a-2",
        "/tmp/worker-a-2",
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

    let client = reqwest::Client::new();
    let mut events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
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
                && !remote_workers[1].reconnecting
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");
    timeout(Duration::from_secs(5), async {
        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);
                if event.contains("event: gateway/reconnected") {
                    assert!(event.contains("\"workerId\":"));
                    assert!(event.contains(&worker_a));
                    return;
                }
            }
        }
    })
    .await
    .expect("reconnected SSE event should arrive before v2 client connects");
    let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("v2 client should connect after worker reconnect");

    let first_started: AppServerThreadStartResponse = v2_client
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
        .expect("first thread/start should succeed after worker reconnect");
    assert_eq!(first_started.thread.id, "thread-worker-a-2");

    let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput { request_id, params }) =
        timeout(Duration::from_secs(5), v2_client.next_event())
            .await
            .expect("server request should arrive after multi-worker reconnect")
            .expect("event stream should stay open after multi-worker reconnect")
    else {
        panic!("expected tool/requestUserInput after multi-worker reconnect");
    };
    assert_eq!(params.thread_id, "thread-worker-a-2");
    assert_eq!(params.turn_id, "turn-worker-a-2");
    assert_eq!(params.item_id, "tool-call-worker-a-2");
    let mut answers = HashMap::new();
    answers.insert(
        "mode".to_string(),
        ToolRequestUserInputAnswer {
            answers: vec!["safe".to_string()],
        },
    );
    v2_client
        .resolve_server_request(
            request_id,
            serde_json::to_value(ToolRequestUserInputResponse { answers })
                .expect("server request response should serialize"),
        )
        .await
        .expect("server request should resolve after multi-worker reconnect");

    let recovered_turn_started: TurnStartResponse = v2_client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello recovered worker".to_string(),
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
        .expect("turn/start should succeed after multi-worker reconnect");
    assert_eq!(recovered_turn_started.turn.id, "turn-thread-worker-a-2");
    assert_eq!(recovered_turn_started.turn.status, TurnStatus::InProgress);

    let mut recovered_turn_coverage = TurnStreamingCoverage::default();
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
        {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after multi-worker reconnect");
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
                    && notification.turn.id == "turn-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-thread-worker-a-2")
                    && notification.run.id == "hook-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_hook_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-thread-worker-a-2" && text == "streaming answer in progress"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "hello from recovered worker" =>
                {
                    recovered_turn_coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "summary thread-worker-a-2"
                    && notification.summary_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "reasoning thread-worker-a-2"
                    && notification.content_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "stdout thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_command_output_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "patch thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_file_change_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-thread-worker-a-2")
                    && notification.run.id == "hook-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_hook_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-thread-worker-a-2" && text == "streaming answer completed"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == "turn-thread-worker-a-2"
                    && notification.turn.status == TurnStatus::Completed =>
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
        "turn notifications should fan in after multi-worker reconnect: {recovered_turn_coverage:?}"
    );

    let second_started: AppServerThreadStartResponse = v2_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
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
        .expect("second thread/start should succeed after worker reconnect");
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let listed: AppServerThreadListResponse = v2_client
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
        .expect("thread/list should succeed after worker reconnect");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a-2"]
    );

    let first_read: AppServerThreadReadResponse = v2_client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to recovered worker");
    assert_eq!(first_read.thread.id, "thread-worker-a-2");
    assert_eq!(first_read.thread.preview, "/tmp/worker-a-2");

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}
