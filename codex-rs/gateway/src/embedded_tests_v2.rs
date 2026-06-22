use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_v2_restore.rs"]
mod embedded_tests_v2_restore;

#[path = "embedded_tests_v2_restore_fail_closed.rs"]
mod embedded_tests_v2_restore_fail_closed;

#[path = "embedded_tests_v2_late.rs"]
mod embedded_tests_v2_late;

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_turn_steer_and_interrupt_have_no_account_replacement_over_v2()
 {
    for method in ["turn/steer", "turn/interrupt"] {
        let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
            "worker-a",
            WorkerAAccountCapacity::Available,
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_legacy_account_handoff_server(
            "worker-b",
            WorkerAAccountCapacity::Available,
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
                            account_id: Some("acct-a".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,

                            auth_token: None,
                            account_id: Some("acct-b".to_string()),
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
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let _worker_a_thread: AppServerThreadStartResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/worker-a".to_string()),
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
            }),
        )
        .await
        .expect("first thread/start should finish in time")
        .expect("first thread/start should register worker A scope");

        let worker_b_thread: AppServerThreadStartResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/worker-b".to_string()),
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
            }),
        )
        .await
        .expect("second thread/start should finish in time")
        .expect("second thread/start should register worker B scope");

        let turn_start_response: TurnStartResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(3),
                params: TurnStartParams {
                    thread_id: worker_b_thread.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "seed active turn before exhaustion".to_string(),
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
            }),
        )
        .await
        .expect("turn/start should finish in time before exhaustion")
        .expect("turn/start should succeed before exhaustion");
        let turn_id = turn_start_response.turn.id.clone();

        timeout(
            Duration::from_secs(5),
            client.request_typed::<GetAccountRateLimitsResponse>(
                ClientRequest::GetAccountRateLimits {
                    request_id: RequestId::Integer(4),
                    params: None,
                },
            ),
        )
        .await
        .expect("account/rateLimits/read should finish in time")
        .expect("account/rateLimits/read should mark worker B exhausted");

        let thread_id = worker_b_thread.thread.id.clone();
        let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
        let handoff_event_task = tokio::spawn({
            let method = method.to_string();
            async move {
                let mut events_response = reqwest::Client::new()
                    .get(handoff_event_url)
                    .send()
                    .await
                    .expect("event stream response");
                assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                        if event.contains("event: gateway/accountActiveThreadHandoffFailed")
                            && event.contains(&format!("\"method\":\"{method}\""))
                        {
                            return event;
                        }
                    }
                }
            }
        });
        sleep(Duration::from_millis(100)).await;

        let request_error = match method {
            "turn/steer" => {
                let result: Result<TurnSteerResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::TurnSteer {
                        request_id: RequestId::Integer(5),
                        params: TurnSteerParams {
                            thread_id: thread_id.clone(),
                            input: vec![UserInput::Text {
                                text: "keep steering".to_string(),
                                text_elements: Vec::new(),
                            }],
                            responsesapi_client_metadata: None,
                            expected_turn_id: turn_id.clone(),
                            ..Default::default()
                        },
                    }),
                )
                .await
                .expect("turn/steer fail-closed response should finish in time");
                result.expect_err("turn/steer should fail closed without a replacement account")
            }
            "turn/interrupt" => {
                let result: Result<TurnInterruptResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::TurnInterrupt {
                        request_id: RequestId::Integer(5),
                        params: TurnInterruptParams {
                            thread_id: thread_id.clone(),
                            turn_id: turn_id.clone(),
                        },
                    }),
                )
                .await
                .expect("turn/interrupt fail-closed response should finish in time");
                result.expect_err("turn/interrupt should fail closed without a replacement account")
            }
            _ => unreachable!("unexpected turn control method: {method}"),
        };

        let expected_message = format!(
            "thread {thread_id} is pinned to worker 1 with exhausted account capacity for {method}"
        );
        assert!(
            request_error
                .to_string()
                .contains(expected_message.as_str()),
            "{request_error}"
        );

        let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
            .await
            .expect("timed out waiting for turn control handoff failure event")
            .expect("handoff event task should finish");
        assert!(handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
        assert!(handoff_event.contains("\"tenantId\":\"default\""));
        assert!(handoff_event.contains("\"projectId\":null"));
        assert!(handoff_event.contains(&format!("\"method\":\"{method}\"")));
        assert!(handoff_event.contains(&format!("\"threadId\":\"{thread_id}\"")));
        assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
        assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }
}

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_thread_id_handoff_has_no_account_replacement() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Exhausted,
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-b",
        WorkerAAccountCapacity::Available,
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
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: Some("acct-b".to_string()),
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
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let _worker_a_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-a".to_string()),
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
        }),
    )
    .await
    .expect("first thread/start should finish in time")
    .expect("first thread/start should register worker A scope");

    let worker_b_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-b".to_string()),
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
        }),
    )
    .await
    .expect("second thread/start should finish in time")
    .expect("second thread/start should register worker B scope");

    timeout(
        Duration::from_secs(5),
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark every worker exhausted");

    let thread_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let thread_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(thread_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let resume_result: Result<ThreadResumeResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: worker_b_thread.thread.id.clone(),
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
        }),
    )
    .await
    .expect("thread/resume fail-closed response should finish in time");
    let resume_error =
        resume_result.expect_err("thread/resume should fail closed without a replacement account");
    assert!(
            resume_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
            ),
            "{resume_error}"
        );

    let thread_handoff_event = timeout(Duration::from_secs(5), thread_handoff_event_task)
        .await
        .expect("timed out waiting for thread handoff failure event")
        .expect("thread handoff failure event task should finish");
    assert!(thread_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(thread_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(thread_handoff_event.contains("\"projectId\":null"));
    assert!(thread_handoff_event.contains("\"method\":\"thread/resume\""));
    assert!(thread_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(thread_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(thread_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(thread_handoff_event.contains("no replacement worker restored the context"));

    let fork_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let fork_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(fork_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/fork\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let fork_result: Result<ThreadForkResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: worker_b_thread.thread.id.clone(),
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
                ephemeral: true,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("thread/fork fail-closed response should finish in time");
    let fork_error =
        fork_result.expect_err("thread/fork should fail closed without a replacement account");
    assert!(
            fork_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
            ),
            "{fork_error}"
        );

    let fork_handoff_event = timeout(Duration::from_secs(5), fork_handoff_event_task)
        .await
        .expect("timed out waiting for fork handoff failure event")
        .expect("fork handoff failure event task should finish");
    assert!(fork_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(fork_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(fork_handoff_event.contains("\"projectId\":null"));
    assert!(fork_handoff_event.contains("\"method\":\"thread/fork\""));
    assert!(fork_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(fork_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(fork_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(fork_handoff_event.contains("no replacement worker restored the context"));

    let read_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let read_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(read_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/read\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let read_result: Result<AppServerThreadReadResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read fail-closed response should finish in time");
    let read_error =
        read_result.expect_err("thread/read should fail closed without a replacement account");
    assert!(
            read_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
            ),
            "{read_error}"
        );

    let read_handoff_event = timeout(Duration::from_secs(5), read_handoff_event_task)
        .await
        .expect("timed out waiting for thread/read handoff failure event")
        .expect("thread/read handoff failure event task should finish");
    assert!(read_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(read_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(read_handoff_event.contains("\"projectId\":null"));
    assert!(read_handoff_event.contains("\"method\":\"thread/read\""));
    assert!(read_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(read_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(read_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(read_handoff_event.contains("no replacement worker restored the context"));

    let rollback_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let rollback_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(rollback_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/rollback\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let rollback_result: Result<ThreadRollbackResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(7),
            params: ThreadRollbackParams {
                thread_id: worker_b_thread.thread.id.clone(),
                num_turns: 1,
            },
        }),
    )
    .await
    .expect("thread/rollback fail-closed response should finish in time");
    let rollback_error = rollback_result
        .expect_err("thread/rollback should fail closed without a replacement account");
    assert!(
            rollback_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/rollback, and no replacement worker restored the context"
            ),
            "{rollback_error}"
        );

    let rollback_handoff_event = timeout(Duration::from_secs(5), rollback_handoff_event_task)
        .await
        .expect("timed out waiting for thread/rollback handoff failure event")
        .expect("thread/rollback handoff failure event task should finish");
    assert!(rollback_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(rollback_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(rollback_handoff_event.contains("\"projectId\":null"));
    assert!(rollback_handoff_event.contains("\"method\":\"thread/rollback\""));
    assert!(
        rollback_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(rollback_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(rollback_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(rollback_handoff_event.contains("no replacement worker restored the context"));

    let archive_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let archive_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(archive_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/archive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let archive_result: Result<ThreadArchiveResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(8),
            params: ThreadArchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/archive fail-closed response should finish in time");
    let archive_error = archive_result
        .expect_err("thread/archive should fail closed without a replacement account");
    assert!(
            archive_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/archive, and no replacement worker restored the context"
            ),
            "{archive_error}"
        );

    let archive_handoff_event = timeout(Duration::from_secs(5), archive_handoff_event_task)
        .await
        .expect("timed out waiting for thread/archive handoff failure event")
        .expect("thread/archive handoff event task should finish");
    assert!(archive_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(archive_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(archive_handoff_event.contains("\"projectId\":null"));
    assert!(archive_handoff_event.contains("\"method\":\"thread/archive\""));
    assert!(
        archive_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(archive_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(archive_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(archive_handoff_event.contains("no replacement worker restored the context"));

    let unarchive_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let unarchive_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(unarchive_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/unarchive\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let unarchive_result: Result<ThreadUnarchiveResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(9),
            params: ThreadUnarchiveParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/unarchive fail-closed response should finish in time");
    let unarchive_error = unarchive_result
        .expect_err("thread/unarchive should fail closed without a replacement account");
    assert!(
            unarchive_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context"
            ),
            "{unarchive_error}"
        );

    let unarchive_handoff_event = timeout(Duration::from_secs(5), unarchive_handoff_event_task)
        .await
        .expect("timed out waiting for thread/unarchive handoff failure event")
        .expect("thread/unarchive handoff event task should finish");
    assert!(unarchive_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(unarchive_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(unarchive_handoff_event.contains("\"projectId\":null"));
    assert!(unarchive_handoff_event.contains("\"method\":\"thread/unarchive\""));
    assert!(
        unarchive_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(unarchive_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(unarchive_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(unarchive_handoff_event.contains("no replacement worker restored the context"));

    let metadata_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let metadata_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(metadata_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/metadata/update\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let metadata_result: Result<ThreadMetadataUpdateResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: worker_b_thread.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        }),
    )
    .await
    .expect("thread/metadata/update fail-closed response should finish in time");
    let metadata_error = metadata_result
        .expect_err("thread/metadata/update should fail closed without a replacement account");
    assert!(
            metadata_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/metadata/update, and no replacement worker restored the context"
            ),
            "{metadata_error}"
        );

    let metadata_handoff_event = timeout(Duration::from_secs(5), metadata_handoff_event_task)
        .await
        .expect("timed out waiting for thread/metadata/update handoff failure event")
        .expect("thread/metadata/update handoff event task should finish");
    assert!(metadata_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(metadata_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(metadata_handoff_event.contains("\"projectId\":null"));
    assert!(metadata_handoff_event.contains("\"method\":\"thread/metadata/update\""));
    assert!(
        metadata_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(metadata_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(metadata_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(metadata_handoff_event.contains("no replacement worker restored the context"));

    let turns_list_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let turns_list_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(turns_list_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/turns/list\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let turns_list_result: Result<ThreadTurnsListResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(11),
            params: ThreadTurnsListParams {
                thread_id: worker_b_thread.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        }),
    )
    .await
    .expect("thread/turns/list fail-closed response should finish in time");
    let turns_list_error = turns_list_result
        .expect_err("thread/turns/list should fail closed without a replacement account");
    assert!(
            turns_list_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context"
            ),
            "{turns_list_error}"
        );

    let turns_list_handoff_event = timeout(Duration::from_secs(5), turns_list_handoff_event_task)
        .await
        .expect("timed out waiting for thread/turns/list handoff failure event")
        .expect("thread/turns/list handoff event task should finish");
    assert!(turns_list_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(turns_list_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(turns_list_handoff_event.contains("\"projectId\":null"));
    assert!(turns_list_handoff_event.contains("\"method\":\"thread/turns/list\""));
    assert!(
        turns_list_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(turns_list_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(turns_list_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(turns_list_handoff_event.contains("no replacement worker restored the context"));

    let increment_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let increment_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(increment_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/increment_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let increment_result: Result<ThreadIncrementElicitationResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(12),
            params: ThreadIncrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/increment_elicitation fail-closed response should finish in time");
    let increment_error = increment_result.expect_err(
        "thread/increment_elicitation should fail closed without a replacement account",
    );
    assert!(
            increment_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context"
            ),
            "{increment_error}"
        );

    let increment_handoff_event = timeout(Duration::from_secs(5), increment_handoff_event_task)
        .await
        .expect("timed out waiting for thread/increment_elicitation handoff failure event")
        .expect("thread/increment_elicitation handoff event task should finish");
    assert!(increment_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(increment_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(increment_handoff_event.contains("\"projectId\":null"));
    assert!(increment_handoff_event.contains("\"method\":\"thread/increment_elicitation\""));
    assert!(
        increment_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(increment_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(increment_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(increment_handoff_event.contains("no replacement worker restored the context"));

    let decrement_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let decrement_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(decrement_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/decrement_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let decrement_result: Result<ThreadDecrementElicitationResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(13),
            params: ThreadDecrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/decrement_elicitation fail-closed response should finish in time");
    let decrement_error = decrement_result.expect_err(
        "thread/decrement_elicitation should fail closed without a replacement account",
    );
    assert!(
            decrement_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/decrement_elicitation, and no replacement worker restored the context"
            ),
            "{decrement_error}"
        );

    let decrement_handoff_event = timeout(Duration::from_secs(5), decrement_handoff_event_task)
        .await
        .expect("timed out waiting for thread/decrement_elicitation handoff failure event")
        .expect("thread/decrement_elicitation handoff event task should finish");
    assert!(decrement_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(decrement_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(decrement_handoff_event.contains("\"projectId\":null"));
    assert!(decrement_handoff_event.contains("\"method\":\"thread/decrement_elicitation\""));
    assert!(
        decrement_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(decrement_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(decrement_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(decrement_handoff_event.contains("no replacement worker restored the context"));

    let inject_items_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let inject_items_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(inject_items_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"thread/inject_items\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let inject_items_result: Result<ThreadInjectItemsResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(14),
            params: ThreadInjectItemsParams {
                thread_id: worker_b_thread.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                })],
            },
        }),
    )
    .await
    .expect("thread/inject_items fail-closed response should finish in time");
    let inject_items_error = inject_items_result
        .expect_err("thread/inject_items should fail closed without a replacement account");
    assert!(
            inject_items_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for thread/inject_items, and no replacement worker restored the context"
            ),
            "{inject_items_error}"
        );

    let inject_items_handoff_event =
        timeout(Duration::from_secs(5), inject_items_handoff_event_task)
            .await
            .expect("timed out waiting for thread/inject_items handoff failure event")
            .expect("thread/inject_items handoff event task should finish");
    assert!(inject_items_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(inject_items_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(inject_items_handoff_event.contains("\"projectId\":null"));
    assert!(inject_items_handoff_event.contains("\"method\":\"thread/inject_items\""));
    assert!(
        inject_items_handoff_event
            .contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(inject_items_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(inject_items_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(inject_items_handoff_event.contains("no replacement worker restored the context"));

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_rollout_path_has_no_account_replacement() {
    let worker_a = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-a",
        WorkerAAccountCapacity::Exhausted,
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_legacy_account_handoff_server(
        "worker-b",
        WorkerAAccountCapacity::Available,
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
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: Some("acct-b".to_string()),
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
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let _worker_a_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-a".to_string()),
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
        }),
    )
    .await
    .expect("first thread/start should finish in time")
    .expect("first thread/start should register worker A scope");

    let worker_b_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-b".to_string()),
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
        }),
    )
    .await
    .expect("second thread/start should finish in time")
    .expect("second thread/start should register worker B scope");
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .expect("worker B thread should expose rollout path");

    timeout(
        Duration::from_secs(5),
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark every worker exhausted");

    let path_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let path_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(path_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountPathHandoffFailed") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let summary_result: Result<GetConversationSummaryResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(4),
            params: GetConversationSummaryParams::RolloutPath {
                rollout_path: worker_b_rollout_path.clone(),
            },
        }),
    )
    .await
    .expect("getConversationSummary fail-closed response should finish in time");
    let summary_error = summary_result
        .expect_err("getConversationSummary should fail closed without a replacement account");
    assert!(
            summary_error.to_string().contains(
                "thread path /tmp/worker-b/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
            ),
            "{summary_error}"
        );

    let path_handoff_event = timeout(Duration::from_secs(5), path_handoff_event_task)
        .await
        .expect("timed out waiting for path handoff failure event")
        .expect("path handoff failure event task should finish");
    assert!(path_handoff_event.contains("event: gateway/accountPathHandoffFailed"));
    assert!(path_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(path_handoff_event.contains("\"projectId\":null"));
    assert!(path_handoff_event.contains("\"method\":\"getConversationSummary\""));
    assert!(path_handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(path_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(path_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(path_handoff_event.contains("no replacement worker restored the context"));

    let resume_path_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let resume_path_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(resume_path_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountPathHandoffFailed")
                    && event.contains("\"method\":\"thread/resume\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let resume_path_result: Result<ThreadResumeResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(5),
            params: ThreadResumeParams {
                thread_id: worker_b_thread.thread.id.clone(),
                history: None,
                path: Some(worker_b_rollout_path.clone()),
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
        }),
    )
    .await
    .expect("path-based thread/resume fail-closed response should finish in time");
    let resume_path_error = resume_path_result
        .expect_err("path-based thread/resume should fail closed without a replacement account");
    assert!(
            resume_path_error.to_string().contains(
                "thread path /tmp/worker-b/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
            ),
            "{resume_path_error}"
        );

    let resume_path_handoff_event = timeout(Duration::from_secs(5), resume_path_handoff_event_task)
        .await
        .expect("timed out waiting for path resume handoff failure event")
        .expect("path resume handoff failure event task should finish");
    assert!(resume_path_handoff_event.contains("event: gateway/accountPathHandoffFailed"));
    assert!(resume_path_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(resume_path_handoff_event.contains("\"projectId\":null"));
    assert!(resume_path_handoff_event.contains("\"method\":\"thread/resume\""));
    assert!(resume_path_handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(resume_path_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(resume_path_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(resume_path_handoff_event.contains("no replacement worker restored the context"));

    let fork_path_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let fork_path_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(fork_path_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountPathHandoffFailed")
                    && event.contains("\"method\":\"thread/fork\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let fork_path_result: Result<ThreadForkResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(6),
            params: ThreadForkParams {
                thread_id: worker_b_thread.thread.id.clone(),
                path: Some(worker_b_rollout_path.clone()),
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
                ephemeral: true,
                ..Default::default()
            },
        }),
    )
    .await
    .expect("path-based thread/fork fail-closed response should finish in time");
    let fork_path_error = fork_path_result
        .expect_err("path-based thread/fork should fail closed without a replacement account");
    assert!(
            fork_path_error.to_string().contains(
                "thread path /tmp/worker-b/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
            ),
            "{fork_path_error}"
        );

    let fork_path_handoff_event = timeout(Duration::from_secs(5), fork_path_handoff_event_task)
        .await
        .expect("timed out waiting for path fork handoff failure event")
        .expect("path fork handoff failure event task should finish");
    assert!(fork_path_handoff_event.contains("event: gateway/accountPathHandoffFailed"));
    assert!(fork_path_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(fork_path_handoff_event.contains("\"projectId\":null"));
    assert!(fork_path_handoff_event.contains("\"method\":\"thread/fork\""));
    assert!(fork_path_handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(fork_path_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(fork_path_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(fork_path_handoff_event.contains("no replacement worker restored the context"));

    let thread_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let thread_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(thread_handoff_event_url)
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

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

                if event.contains("event: gateway/accountThreadHandoffFailed")
                    && event.contains("\"method\":\"getConversationSummary\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let summary_by_thread_result: Result<GetConversationSummaryResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(7),
            params: GetConversationSummaryParams::ThreadId {
                conversation_id: ThreadId::from_string(&worker_b_thread.thread.id)
                    .expect("worker B thread id should parse"),
            },
        }),
    )
    .await
    .expect("getConversationSummary by thread id fail-closed response should finish in time");
    let summary_by_thread_error = summary_by_thread_result.expect_err(
        "getConversationSummary by thread id should fail closed without a replacement account",
    );
    assert!(
            summary_by_thread_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
            ),
            "{summary_by_thread_error}"
        );

    let thread_handoff_event = timeout(Duration::from_secs(5), thread_handoff_event_task)
        .await
        .expect("timed out waiting for conversation summary thread handoff failure event")
        .expect("thread handoff failure event task should finish");
    assert!(thread_handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(thread_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(thread_handoff_event.contains("\"projectId\":null"));
    assert!(thread_handoff_event.contains("\"method\":\"getConversationSummary\""));
    assert!(thread_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(thread_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(thread_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(thread_handoff_event.contains("no replacement worker restored the context"));

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
