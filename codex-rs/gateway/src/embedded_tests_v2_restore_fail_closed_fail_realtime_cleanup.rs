use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_fails_closed_when_thread_realtime_controls_have_no_account_replacement_over_v2()
 {
    for method in [
        "thread/realtime/start",
        "thread/realtime/appendText",
        "thread/realtime/appendAudio",
        "thread/realtime/stop",
    ] {
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

        timeout(
            Duration::from_secs(5),
            client.request_typed::<GetAccountRateLimitsResponse>(
                ClientRequest::GetAccountRateLimits {
                    request_id: RequestId::Integer(3),
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
            "thread/realtime/start" => {
                let result: Result<ThreadRealtimeStartResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadRealtimeStart {
                        request_id: RequestId::Integer(4),
                        params: ThreadRealtimeStartParams {
                            thread_id: thread_id.clone(),
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
                    }),
                )
                .await
                .expect("thread/realtime/start fail-closed response should finish in time");
                result.expect_err(
                    "thread/realtime/start should fail closed without a replacement account",
                )
            }
            "thread/realtime/appendText" => {
                let result: Result<ThreadRealtimeAppendTextResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadRealtimeAppendText {
                        request_id: RequestId::Integer(4),
                        params: ThreadRealtimeAppendTextParams {
                            thread_id: thread_id.clone(),
                            text: "hello".to_string(),
                            ..Default::default()
                        },
                    }),
                )
                .await
                .expect("thread/realtime/appendText fail-closed response should finish in time");
                result.expect_err(
                    "thread/realtime/appendText should fail closed without a replacement account",
                )
            }
            "thread/realtime/appendAudio" => {
                let result: Result<ThreadRealtimeAppendAudioResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadRealtimeAppendAudio {
                        request_id: RequestId::Integer(4),
                        params: ThreadRealtimeAppendAudioParams {
                            thread_id: thread_id.clone(),
                            audio: ThreadRealtimeAudioChunk {
                                data: "AQID".to_string(),
                                sample_rate: 24_000,
                                num_channels: 1,
                                samples_per_channel: Some(3),
                                item_id: Some("item-audio".to_string()),
                            },
                        },
                    }),
                )
                .await
                .expect("thread/realtime/appendAudio fail-closed response should finish in time");
                result.expect_err(
                    "thread/realtime/appendAudio should fail closed without a replacement account",
                )
            }
            "thread/realtime/stop" => {
                let result: Result<ThreadRealtimeStopResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadRealtimeStop {
                        request_id: RequestId::Integer(4),
                        params: ThreadRealtimeStopParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect("thread/realtime/stop fail-closed response should finish in time");
                result.expect_err(
                    "thread/realtime/stop should fail closed without a replacement account",
                )
            }
            _ => unreachable!("unexpected realtime control method: {method}"),
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
            .expect("timed out waiting for realtime handoff failure event")
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
async fn remote_multi_worker_fails_closed_when_thread_shell_command_and_background_cleanup_have_no_account_replacement_over_v2()
 {
    for method in ["thread/shellCommand", "thread/backgroundTerminals/clean"] {
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

        timeout(
            Duration::from_secs(5),
            client.request_typed::<GetAccountRateLimitsResponse>(
                ClientRequest::GetAccountRateLimits {
                    request_id: RequestId::Integer(3),
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
            "thread/shellCommand" => {
                let result: Result<ThreadShellCommandResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadShellCommand {
                        request_id: RequestId::Integer(4),
                        params: ThreadShellCommandParams {
                            thread_id: thread_id.clone(),
                            command: "echo hello".to_string(),
                        },
                    }),
                )
                .await
                .expect("thread/shellCommand fail-closed response should finish in time");
                result.expect_err(
                    "thread/shellCommand should fail closed without a replacement account",
                )
            }
            "thread/backgroundTerminals/clean" => {
                let result: Result<ThreadBackgroundTerminalsCleanResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
                        request_id: RequestId::Integer(4),
                        params: ThreadBackgroundTerminalsCleanParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect(
                    "thread/backgroundTerminals/clean fail-closed response should finish in time",
                );
                result.expect_err(
                        "thread/backgroundTerminals/clean should fail closed without a replacement account",
                    )
            }
            _ => unreachable!("unexpected thread-scoped cleanup method: {method}"),
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
            .expect("timed out waiting for thread-scoped cleanup handoff failure event")
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
