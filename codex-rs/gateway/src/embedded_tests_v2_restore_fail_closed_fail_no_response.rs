use super::*;
use pretty_assertions::assert_eq;

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
