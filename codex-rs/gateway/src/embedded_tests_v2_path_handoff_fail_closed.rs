use super::*;
use pretty_assertions::assert_eq;

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
