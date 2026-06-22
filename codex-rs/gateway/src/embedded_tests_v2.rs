use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_restores_thread_turns_list_after_account_exhaustion_over_v2() {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/turns/list\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let turns_response: ThreadTurnsListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(4),
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
    .expect("thread/turns/list should finish in time")
    .expect("thread/turns/list should restore through the replacement worker");
    assert_eq!(turns_response.data.len(), 1);
    assert_eq!(turns_response.data[0].id, "turn-worker-a-restored");
    assert_eq!(turns_response.next_cursor, None);
    assert_eq!(turns_response.backwards_cursor, None);

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/turns/list handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/turns/list\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after turns list should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_thread_increment_elicitation_after_account_exhaustion_over_v2()
 {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/increment_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let increment_response: ThreadIncrementElicitationResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(4),
            params: ThreadIncrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/increment_elicitation should finish in time")
    .expect("thread/increment_elicitation should restore through the replacement worker");
    assert_eq!(
        increment_response,
        ThreadIncrementElicitationResponse {
            count: 7,
            paused: true,
        }
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/increment_elicitation handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/increment_elicitation\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after increment elicitation should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_thread_decrement_elicitation_after_account_exhaustion_over_v2()
 {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/decrement_elicitation\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let decrement_response: ThreadDecrementElicitationResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(4),
            params: ThreadDecrementElicitationParams {
                thread_id: worker_b_thread.thread.id.clone(),
            },
        }),
    )
    .await
    .expect("thread/decrement_elicitation should finish in time")
    .expect("thread/decrement_elicitation should restore through the replacement worker");
    assert_eq!(
        decrement_response,
        ThreadDecrementElicitationResponse {
            count: 6,
            paused: false,
        }
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/decrement_elicitation handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/decrement_elicitation\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after decrement elicitation should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_thread_inject_items_after_account_exhaustion_over_v2() {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/inject_items\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let inject_response: ThreadInjectItemsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(4),
            params: ThreadInjectItemsParams {
                thread_id: worker_b_thread.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "replacement seed item"}],
                })],
            },
        }),
    )
    .await
    .expect("thread/inject_items should finish in time")
    .expect("thread/inject_items should restore through the replacement worker");
    assert_eq!(inject_response, ThreadInjectItemsResponse {});

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/inject_items handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/inject_items\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after item injection should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_thread_name_set_after_account_exhaustion_over_v2() {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/name/set\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let renamed = "Restored Worker Thread".to_string();
    let rename_response: ThreadSetNameResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(4),
            params: ThreadSetNameParams {
                thread_id: worker_b_thread.thread.id.clone(),
                name: renamed.clone(),
            },
        }),
    )
    .await
    .expect("thread/name/set should finish in time")
    .expect("thread/name/set should restore through the replacement worker");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/name/set handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/name/set\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after rename should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_thread_memory_mode_after_account_exhaustion_over_v2() {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"thread/memoryMode/set\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let memory_mode_response: ThreadMemoryModeSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(4),
            params: ThreadMemoryModeSetParams {
                thread_id: worker_b_thread.thread.id.clone(),
                mode: ThreadMemoryMode::Disabled,
            },
        }),
    )
    .await
    .expect("thread/memoryMode/set should finish in time")
    .expect("thread/memoryMode/set should restore through the replacement worker");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/memoryMode/set handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/memoryMode/set\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after memory-mode update should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_no_thread_response_controls_after_account_exhaustion_over_v2()
{
    for method in ["thread/unsubscribe", "thread/compact/start"] {
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
        let method = method.to_string();
        let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
        let handoff_event_task = tokio::spawn({
            let method = method.clone();
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

                        if event.contains("event: gateway/accountThreadHandoffSucceeded")
                            && event.contains(&format!("\"method\":\"{method}\""))
                        {
                            return event;
                        }
                    }
                }
            }
        });
        sleep(Duration::from_millis(100)).await;

        match method.as_str() {
            "thread/unsubscribe" => {
                let unsubscribe_response: ThreadUnsubscribeResponse = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadUnsubscribe {
                        request_id: RequestId::Integer(4),
                        params: ThreadUnsubscribeParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect("thread/unsubscribe should finish in time")
                .expect("thread/unsubscribe should restore through the replacement worker");
                assert_eq!(
                    unsubscribe_response,
                    ThreadUnsubscribeResponse {
                        status: ThreadUnsubscribeStatus::Unsubscribed,
                    }
                );
            }
            "thread/compact/start" => {
                let compact_response: ThreadCompactStartResponse = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadCompactStart {
                        request_id: RequestId::Integer(4),
                        params: ThreadCompactStartParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect("thread/compact/start should finish in time")
                .expect("thread/compact/start should restore through the replacement worker");
                assert_eq!(compact_response, ThreadCompactStartResponse {});
            }
            _ => unreachable!("unexpected no-thread-response control method: {method}"),
        }

        let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
            .await
            .expect("timed out waiting for no-thread-response handoff event")
            .expect("handoff event task should finish");
        assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
        assert!(handoff_event.contains("\"tenantId\":\"default\""));
        assert!(handoff_event.contains("\"projectId\":null"));
        assert!(handoff_event.contains(&format!("\"method\":\"{method}\"")));
        assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
        assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
        assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
        assert!(handoff_event.contains("\"replacementWorkerId\":0"));
        assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

        let restored_read: AppServerThreadReadResponse = timeout(
                Duration::from_secs(5),
                client.request_typed(ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(5),
                    params: ThreadReadParams {
                        thread_id: thread_id.clone(),
                        include_turns: false,
                    },
                }),
            )
            .await
            .expect("thread/read should finish in time")
            .expect("thread/read after no-thread-response restoration should stay on the replacement worker");
        assert_eq!(restored_read.thread.id, thread_id);
        assert_eq!(
            restored_read.thread.cwd.as_ref().to_string_lossy(),
            "/tmp/worker-a-read"
        );

        let healthz_response = reqwest::get(format!("http://{}/healthz", server.local_addr()))
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        let success_metric = match method.as_str() {
            "thread/unsubscribe" => "thread_unsubscribe_handoff_success",
            "thread/compact/start" => "thread_compact_start_handoff_success",
            _ => unreachable!("unexpected no-thread-response control method: {method}"),
        };
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_counts
                .get("exhausted"),
            Some(&1)
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_counts
                .get(success_metric),
            Some(&1)
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_worker_counts
                .iter()
                .find(|counts| counts.worker_id == 0)
                .map(|counts| counts.event_counts.get(success_metric)),
            Some(Some(&1))
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_worker_counts
                .iter()
                .find(|counts| counts.worker_id == 1)
                .map(|counts| counts.event_counts.get("exhausted")),
            Some(Some(&1))
        );
        assert_eq!(
            health.v2_connections.last_account_capacity_event.as_deref(),
            Some(success_metric)
        );
        assert_eq!(
            health.v2_connections.last_account_capacity_event_worker_id,
            Some(0)
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_tenant_id
                .as_deref(),
            Some("default")
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_project_id
                .as_deref(),
            None
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_reason
                .as_deref(),
            Some("thread id request restored on a replacement account-backed worker")
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_at
                .is_some(),
            true
        );

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
async fn remote_multi_worker_fails_closed_when_no_thread_response_controls_have_no_account_replacement()
 {
    for method in ["thread/unsubscribe", "thread/compact/start"] {
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
        let method = method.to_string();
        let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
        let handoff_event_task = tokio::spawn({
            let method = method.clone();
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

                        if event.contains("event: gateway/accountThreadHandoffFailed")
                            && event.contains(&format!("\"method\":\"{method}\""))
                        {
                            return event;
                        }
                    }
                }
            }
        });
        sleep(Duration::from_millis(100)).await;

        let request_error = match method.as_ref() {
            "thread/unsubscribe" => {
                let result: Result<ThreadUnsubscribeResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadUnsubscribe {
                        request_id: RequestId::Integer(4),
                        params: ThreadUnsubscribeParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect("thread/unsubscribe fail-closed response should finish in time");
                result.expect_err(
                    "thread/unsubscribe should fail closed without a replacement account",
                )
            }
            "thread/compact/start" => {
                let result: Result<ThreadCompactStartResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::ThreadCompactStart {
                        request_id: RequestId::Integer(4),
                        params: ThreadCompactStartParams {
                            thread_id: thread_id.clone(),
                        },
                    }),
                )
                .await
                .expect("thread/compact/start fail-closed response should finish in time");
                result.expect_err(
                    "thread/compact/start should fail closed without a replacement account",
                )
            }
            _ => unreachable!("unexpected no-thread-response control method: {method}"),
        };

        let expected_message = format!(
            "thread {thread_id} is pinned to worker 1 with exhausted account capacity for {method}, and no replacement worker restored the context"
        );
        assert!(
            request_error
                .to_string()
                .contains(expected_message.as_str()),
            "{request_error}"
        );

        let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
            .await
            .expect("timed out waiting for no-thread-response handoff failure event")
            .expect("handoff event task should finish");
        assert!(handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
        assert!(handoff_event.contains("\"tenantId\":\"default\""));
        assert!(handoff_event.contains("\"projectId\":null"));
        assert!(handoff_event.contains(&format!("\"method\":\"{method}\"")));
        assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
        assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
        assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
        assert!(handoff_event.contains("no replacement worker restored the context"));

        let healthz_response = reqwest::get(format!("http://{}/healthz", server.local_addr()))
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        let failure_metric = match method.as_str() {
            "thread/unsubscribe" => "thread_unsubscribe_handoff_failure",
            "thread/compact/start" => "thread_compact_start_handoff_failure",
            _ => unreachable!("unexpected no-thread-response control method: {method}"),
        };
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_counts
                .get("exhausted"),
            Some(&2)
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_counts
                .get(failure_metric),
            Some(&1)
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_worker_counts
                .iter()
                .find(|counts| counts.worker_id == 0)
                .map(|counts| counts.event_counts.get("exhausted")),
            Some(Some(&1))
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_worker_counts
                .iter()
                .find(|counts| counts.worker_id == 1)
                .map(|counts| counts.event_counts.get("exhausted")),
            Some(Some(&1))
        );
        assert_eq!(
            health
                .v2_connections
                .account_capacity_event_worker_counts
                .iter()
                .find(|counts| counts.worker_id == 1)
                .map(|counts| counts.event_counts.get(failure_metric)),
            Some(Some(&1))
        );
        assert_eq!(
            health.v2_connections.last_account_capacity_event.as_deref(),
            Some(failure_metric)
        );
        assert_eq!(
            health.v2_connections.last_account_capacity_event_worker_id,
            Some(1)
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_tenant_id
                .as_deref(),
            Some("default")
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_project_id
                .as_deref(),
            None
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_reason
                .as_deref(),
            Some(expected_message.as_str())
        );
        assert_eq!(
            health
                .v2_connections
                .last_account_capacity_event_at
                .is_some(),
            true
        );

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
async fn remote_multi_worker_fails_closed_when_review_start_has_no_account_replacement_over_v2() {
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
        client.request_typed::<GetAccountRateLimitsResponse>(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");

    let review_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let review_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(review_handoff_event_url)
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
                    && event.contains("\"method\":\"review/start\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let review_result: Result<ReviewStartResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(4),
            params: ReviewStartParams {
                thread_id: worker_b_thread.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review the current change".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        }),
    )
    .await
    .expect("review/start fail-closed response should finish in time");
    let review_error =
        review_result.expect_err("review/start should fail closed without a replacement account");
    assert!(
            review_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for review/start"
            ),
            "{review_error}"
        );

    let review_handoff_event = timeout(Duration::from_secs(5), review_handoff_event_task)
        .await
        .expect("timed out waiting for review/start handoff failure event")
        .expect("review/start handoff event task should finish");
    assert!(review_handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
    assert!(review_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(review_handoff_event.contains("\"projectId\":null"));
    assert!(review_handoff_event.contains("\"method\":\"review/start\""));
    assert!(review_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(review_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(review_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));

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
async fn remote_multi_worker_fails_closed_when_thread_scoped_mcp_calls_have_no_account_replacement_over_v2()
 {
    for method in ["mcpServer/resource/read", "mcpServer/tool/call"] {
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
            "mcpServer/resource/read" => {
                let result: Result<McpResourceReadResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::McpResourceRead {
                        request_id: RequestId::Integer(4),
                        params: McpResourceReadParams {
                            thread_id: Some(worker_b_thread.thread.id.clone()),
                            server: "remote-mcp".to_string(),
                            uri: "file:///tmp/remote-project/context.md".to_string(),
                        },
                    }),
                )
                .await
                .expect("mcpServer/resource/read fail-closed response should finish in time");
                result.expect_err(
                    "mcpServer/resource/read should fail closed without a replacement account",
                )
            }
            "mcpServer/tool/call" => {
                let result: Result<McpServerToolCallResponse, _> = timeout(
                    Duration::from_secs(5),
                    client.request_typed(ClientRequest::McpServerToolCall {
                        request_id: RequestId::Integer(4),
                        params: McpServerToolCallParams {
                            thread_id: worker_b_thread.thread.id.clone(),
                            server: "remote-mcp".to_string(),
                            tool: "lookup".to_string(),
                            arguments: Some(serde_json::json!({
                                "query": "gateway",
                            })),
                            meta: None,
                        },
                    }),
                )
                .await
                .expect("mcpServer/tool/call fail-closed response should finish in time");
                result.expect_err(
                    "mcpServer/tool/call should fail closed without a replacement account",
                )
            }
            _ => unreachable!("unexpected thread-scoped mcp method: {method}"),
        };

        let expected_message = format!(
            "thread {} is pinned to worker 1 with exhausted account capacity for {method}",
            worker_b_thread.thread.id
        );
        assert!(
            request_error
                .to_string()
                .contains(expected_message.as_str()),
            "{request_error}"
        );

        let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
            .await
            .expect("timed out waiting for thread-scoped MCP handoff failure event")
            .expect("handoff event task should finish");
        assert!(handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
        assert!(handoff_event.contains("\"tenantId\":\"default\""));
        assert!(handoff_event.contains("\"projectId\":null"));
        assert!(handoff_event.contains(&format!("\"method\":\"{method}\"")));
        assert!(handoff_event.contains(&format!("\"threadId\":\"{}\"", worker_b_thread.thread.id)));
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

#[tokio::test]
async fn remote_multi_worker_restores_path_resume_after_account_exhaustion_over_v2() {
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
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .clone()
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
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountPathHandoffSucceeded")
                    && event.contains("\"method\":\"thread/resume\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let resumed: ThreadResumeResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
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
    .expect("path-based thread/resume should finish in time")
    .expect("path-based thread/resume should restore through the replacement worker");
    assert_eq!(resumed.thread.id, worker_b_thread.thread.id);
    assert_eq!(resumed.thread.path, Some(worker_b_rollout_path.clone()));
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a");

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for path thread/resume handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountPathHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/resume\""));
    assert!(handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let restored_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read after path resume should stay on the replacement worker");
    assert_eq!(restored_read.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        restored_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

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
async fn remote_multi_worker_restores_path_fork_after_account_exhaustion_over_v2() {
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
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .clone()
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
    .expect("account/rateLimits/read should mark worker B exhausted");

    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
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

                if event.contains("event: gateway/accountPathHandoffSucceeded")
                    && event.contains("\"method\":\"thread/fork\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let forked: ThreadForkResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(4),
            params: ThreadForkParams {
                thread_id: worker_b_thread.thread.id.clone(),
                path: Some(worker_b_rollout_path),
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
    .expect("path-based thread/fork should finish in time")
    .expect("path-based thread/fork should restore through the replacement worker");
    assert_eq!(
        forked.thread.id.to_string(),
        "00000000-0000-0000-0000-0000000000a2"
    );
    assert_eq!(
        forked.thread.forked_from_id.as_ref(),
        Some(&worker_b_thread.thread.id)
    );
    assert_eq!(forked.cwd.as_ref().to_string_lossy(), "/tmp/worker-a-fork");

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for path thread/fork handoff event")
        .expect("handoff event task should finish");
    assert!(handoff_event.contains("event: gateway/accountPathHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"thread/fork\""));
    assert!(handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let forked_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: forked.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read should finish in time")
    .expect("thread/read of path fork should stay on the replacement worker");
    assert_eq!(forked_read.thread.id, forked.thread.id);
    assert_eq!(
        forked_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-fork"
    );

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
async fn remote_multi_worker_restores_legacy_conversation_summary_after_account_exhaustion() {
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
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .expect("worker B thread should expose rollout path");

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(3),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should mark worker B exhausted");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("worker-b"))
            .and_then(|snapshot| snapshot.rate_limit_reached_type),
        Some(codex_app_server_protocol::RateLimitReachedType::RateLimitReached)
    );

    let active_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let active_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(active_handoff_event_url)
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

                if event.contains("event: gateway/accountActiveThreadHandoffFailed") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let active_thread_result: Result<TurnStartResponse, _> = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: worker_b_thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "continue on exhausted account".to_string(),
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
    .expect("turn/start fail-closed response should finish in time");
    let active_thread_error = active_thread_result
        .expect_err("active thread request should fail closed instead of moving context");
    assert!(
            active_thread_error.to_string().contains(
                "thread 00000000-0000-0000-0000-0000000000b2 is pinned to worker 1 with exhausted account capacity for turn/start"
            ),
            "{active_thread_error}"
        );

    let active_handoff_event = timeout(Duration::from_secs(5), active_handoff_event_task)
        .await
        .expect("timed out waiting for active thread handoff failure event")
        .expect("active handoff event task should finish");
    assert!(active_handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
    assert!(active_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(active_handoff_event.contains("\"projectId\":null"));
    assert!(active_handoff_event.contains("\"method\":\"turn/start\""));
    assert!(active_handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(active_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(active_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));

    let event_url = format!("http://{}/v1/events", server.local_addr());
    let event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(event_url)
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

                if event.contains("event: gateway/accountPathHandoffSucceeded") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let summary: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(5),
            params: GetConversationSummaryParams::RolloutPath {
                rollout_path: worker_b_rollout_path.clone(),
            },
        }),
    )
    .await
    .expect("getConversationSummary should finish in time")
    .expect("getConversationSummary should restore through the replacement worker");
    assert_eq!(
        summary.summary.conversation_id.to_string(),
        "00000000-0000-0000-0000-0000000000a1"
    );
    assert_eq!(summary.summary.path, worker_b_rollout_path);
    assert_eq!(summary.summary.preview, "Worker A restored summary");

    let handoff_event = timeout(Duration::from_secs(5), event_task)
        .await
        .expect("timed out waiting for path handoff event")
        .expect("event task should finish");
    assert!(handoff_event.contains("event: gateway/accountPathHandoffSucceeded"));
    assert!(handoff_event.contains("\"tenantId\":\"default\""));
    assert!(handoff_event.contains("\"projectId\":null"));
    assert!(handoff_event.contains("\"method\":\"getConversationSummary\""));
    assert!(handoff_event.contains("\"threadPath\":\"/tmp/worker-b/rollout.jsonl\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let summary_by_thread_handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let summary_by_thread_handoff_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(summary_by_thread_handoff_event_url)
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded")
                    && event.contains("\"method\":\"getConversationSummary\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let summary_by_thread_id: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(6),
            params: GetConversationSummaryParams::ThreadId {
                conversation_id: ThreadId::from_string(&worker_b_thread.thread.id)
                    .expect("worker B thread id should parse"),
            },
        }),
    )
    .await
    .expect("getConversationSummary by thread id should finish in time")
    .expect("getConversationSummary by thread id should restore through the replacement worker");
    assert_eq!(
        summary_by_thread_id.summary.conversation_id.to_string(),
        "00000000-0000-0000-0000-0000000000b2"
    );
    assert_eq!(summary_by_thread_id.summary.path, worker_b_rollout_path);
    assert_eq!(
        summary_by_thread_id.summary.preview,
        "Worker A restored summary"
    );

    let summary_by_thread_handoff_event =
        timeout(Duration::from_secs(5), summary_by_thread_handoff_event_task)
            .await
            .expect("timed out waiting for conversation summary thread handoff event")
            .expect("summary thread handoff event task should finish");
    assert!(
        summary_by_thread_handoff_event.contains("event: gateway/accountThreadHandoffSucceeded")
    );
    assert!(summary_by_thread_handoff_event.contains("\"tenantId\":\"default\""));
    assert!(summary_by_thread_handoff_event.contains("\"projectId\":null"));
    assert!(summary_by_thread_handoff_event.contains("\"method\":\"getConversationSummary\""));
    assert!(
        summary_by_thread_handoff_event
            .contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\"")
    );
    assert!(summary_by_thread_handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(summary_by_thread_handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(summary_by_thread_handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(summary_by_thread_handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    let forked: ThreadForkResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(7),
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
    .expect("thread/fork should finish in time")
    .expect("thread/fork should stay on the replacement worker");
    assert_eq!(
        forked.thread.id.to_string(),
        "00000000-0000-0000-0000-0000000000a2"
    );
    assert_eq!(
        forked.thread.forked_from_id.as_ref(),
        Some(&worker_b_thread.thread.id)
    );
    assert_eq!(forked.cwd.as_ref().to_string_lossy(), "/tmp/worker-a-fork");

    let resumed: ThreadResumeResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(8),
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
    .expect("thread/resume should finish in time")
    .expect("thread/resume should stay on the replacement worker");
    assert_eq!(resumed.thread.id, worker_b_thread.thread.id);
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a");

    let reread: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(9),
            params: ThreadReadParams {
                thread_id: worker_b_thread.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("thread/read after account handoff should finish in time")
    .expect("thread/read after account handoff should stay on the replacement worker");
    assert_eq!(reread.thread.id, worker_b_thread.thread.id);
    assert_eq!(
        reread.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-read"
    );

    let healthz_response = reqwest::get(format!("http://{}/healthz", server.local_addr()))
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    let remote_workers = health.remote_workers.as_ref().expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(
        remote_workers[1].account_capacity,
        crate::api::GatewayAccountCapacityStatus::Exhausted
    );
    assert_eq!(
        remote_workers[1].account_capacity_reason.as_deref(),
        Some("account/rateLimits reported Worker B RateLimitReached")
    );
    assert_eq!(
        remote_workers[1].account_capacity_last_changed_at.is_some(),
        true
    );
    let account_capacity_events = &health.v2_connections.account_capacity_event_counts;
    assert_eq!(
        account_capacity_events.get("active_thread_handoff_failure"),
        Some(&1)
    );
    assert_eq!(
        account_capacity_events.get("path_thread_handoff_success"),
        Some(&1)
    );
    assert_eq!(
        account_capacity_events.get("conversation_summary_handoff_success"),
        Some(&1)
    );
    assert_eq!(
        health.v2_connections.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_success")
    );
    assert_eq!(
        health.v2_connections.last_account_capacity_event_worker_id,
        Some(0)
    );
    assert_eq!(
        health
            .v2_connections
            .last_account_capacity_event_tenant_id
            .as_deref(),
        Some("default")
    );
    assert_eq!(
        health.v2_connections.last_account_capacity_event_project_id,
        None
    );
    assert_eq!(
        health
            .v2_connections
            .last_account_capacity_event_reason
            .as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(
        health
            .v2_connections
            .account_capacity_event_worker_counts
            .iter()
            .find(|counts| counts.worker_id == 0)
            .map(|counts| &counts.event_counts)
            .and_then(|event_counts| event_counts.get("path_thread_handoff_success")),
        Some(&1)
    );
    assert_eq!(
        health
            .v2_connections
            .account_capacity_event_worker_counts
            .iter()
            .find(|counts| counts.worker_id == 1)
            .map(|counts| &counts.event_counts)
            .and_then(|event_counts| event_counts.get("active_thread_handoff_failure")),
        Some(&1)
    );
    assert_eq!(
        health.v2_connections.fail_closed_request_counts,
        vec![crate::api::GatewayV2FailClosedRequestCounts {
            method: "turn/start".to_string(),
            reconnect_backoff_active: false,
            count: 1,
        }]
    );
    assert_eq!(
        health
            .v2_connections
            .last_fail_closed_request_method
            .as_deref(),
        Some("turn/start")
    );
    assert_eq!(
        health
            .v2_connections
            .last_fail_closed_request_reconnect_backoff_active,
        Some(false)
    );
    assert_eq!(
        health.v2_connections.last_fail_closed_request_at.is_some(),
        true
    );

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


#[path = "embedded_tests_v2_late.rs"]
mod embedded_tests_v2_late;
