use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_runtime_retries_thread_start_after_account_capacity_error() {
    let worker_a = start_mock_remote_server_for_thread_start_error(
        Some("secret-token".to_string()),
        JSONRPCErrorError {
            code: 429,
            message: "rate limit reached".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-worker-b",
        "/tmp/worker-b",
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

                        auth_token: Some("secret-token".to_string()),
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: Some("secret-token".to_string()),
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

    let client = reqwest::Client::new();
    let event_client = client.clone();
    let event_url = format!("http://{}/v1/events", server.local_addr());
    let event_task = tokio::spawn(async move {
        let mut events_response = event_client
            .get(event_url)
            .header("x-codex-project-id", "project-a")
            .send()
            .await
            .expect("event stream response");
        assert_eq!(events_response.status(), reqwest::StatusCode::OK);

        let mut buffered = String::new();
        let mut exhausted_event = None;
        let mut cooldown_event = None;
        let mut failover_event = None;
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

                if event.contains("event: gateway/accountCapacityExhausted") {
                    exhausted_event = Some(event);
                } else if event.contains("event: gateway/accountLeaseChanged")
                    && event.contains("\"leaseState\":\"cooldown\"")
                {
                    cooldown_event = Some(event);
                } else if event.contains("event: gateway/accountFailoverSucceeded") {
                    failover_event = Some(event);
                }

                if let (Some(exhausted_event), Some(cooldown_event), Some(failover_event)) = (
                    exhausted_event.as_ref(),
                    cooldown_event.as_ref(),
                    failover_event.as_ref(),
                ) {
                    return (
                        exhausted_event.clone(),
                        cooldown_event.clone(),
                        failover_event.clone(),
                    );
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let thread: ThreadResponse = response.json().await.expect("thread");
    assert_eq!(thread.thread.id, "thread-worker-b");

    let (exhausted_event, cooldown_event, failover_event) =
        timeout(Duration::from_secs(5), event_task)
            .await
            .expect("timed out waiting for account failover events")
            .expect("event task should finish");

    assert_eq!(
        exhausted_event.contains("event: gateway/accountCapacityExhausted"),
        true
    );
    assert_eq!(exhausted_event.contains("\"tenantId\":\"default\""), true);
    assert_eq!(
        exhausted_event.contains("\"projectId\":\"project-a\""),
        true
    );
    assert_eq!(exhausted_event.contains("\"workerId\":0"), true);
    assert_eq!(exhausted_event.contains("\"accountId\":\"acct-a\""), true);
    assert_eq!(
        exhausted_event.contains("\"reason\":\"rate limit reached\""),
        true
    );
    assert_eq!(
        cooldown_event.contains("event: gateway/accountLeaseChanged"),
        true
    );
    assert_eq!(cooldown_event.contains("\"tenantId\":\"default\""), true);
    assert_eq!(cooldown_event.contains("\"projectId\":\"project-a\""), true);
    assert_eq!(cooldown_event.contains("\"workerId\":0"), true);
    assert_eq!(cooldown_event.contains("\"accountId\":\"acct-a\""), true);
    assert_eq!(cooldown_event.contains("\"leaseState\":\"cooldown\""), true);
    assert_eq!(
        cooldown_event.contains("\"reason\":\"rate limit reached\""),
        true
    );
    assert_eq!(
        failover_event.contains("event: gateway/accountFailoverSucceeded"),
        true
    );
    assert_eq!(failover_event.contains("\"tenantId\":\"default\""), true);
    assert_eq!(failover_event.contains("\"projectId\":\"project-a\""), true);
    assert_eq!(failover_event.contains("\"replacementWorkerId\":1"), true);
    assert_eq!(
        failover_event.contains("\"replacementAccountId\":\"acct-b\""),
        true
    );
    assert_eq!(failover_event.contains("\"exhaustedWorkerIds\":[0]"), true);

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(remote_workers[0].account_id.as_deref(), Some("acct-a"));
    assert_eq!(
        remote_workers[0].account_capacity,
        crate::api::GatewayAccountCapacityStatus::Exhausted
    );
    assert_eq!(
        remote_workers[0].account_capacity_reason.as_deref(),
        Some("rate limit reached")
    );
    assert_eq!(
        remote_workers[0].account_capacity_last_changed_at.is_some(),
        true
    );
    assert_eq!(remote_workers[1].account_id.as_deref(), Some("acct-b"));
    assert_eq!(
        health.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "default".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: true,
            account_routing_eligible: true,
        }])
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_restores_thread_read_after_account_capacity_exhaustion() {
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
    let worker_c = start_mock_remote_server_for_thread_start_error(
        None,
        JSONRPCErrorError {
            code: 429,
            message: "rate limit reached".to_string(),
            data: None,
        },
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
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_c,

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

    let client = reqwest::Client::new();
    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/worker-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first thread response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/worker-b".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("second thread response");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(
        second_thread.thread.id,
        "00000000-0000-0000-0000-0000000000b2"
    );

    let handoff_event_client = client.clone();
    let handoff_event_url = format!("http://{}/v1/events", server.local_addr());
    let handoff_event_task = tokio::spawn(async move {
        let mut events_response = handoff_event_client
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

                if event.contains("event: gateway/accountThreadHandoffSucceeded") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let quota_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/quota".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("quota failover thread response");
    assert_eq!(quota_response.status(), reqwest::StatusCode::OK);

    let read_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            second_thread.thread.id
        ))
        .send()
        .await
        .expect("read response");
    assert_eq!(read_response.status(), reqwest::StatusCode::OK);
    let read_thread: ThreadResponse = read_response.json().await.expect("read thread");
    assert_eq!(read_thread.thread.id, second_thread.thread.id);
    assert_eq!(read_thread.thread.preview, "/tmp/worker-a-read");

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/read handoff event")
        .expect("event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffSucceeded"));
    assert!(handoff_event.contains("\"method\":\"thread/read\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("\"replacementWorkerId\":0"));
    assert!(handoff_event.contains("\"replacementAccountId\":\"acct-a\""));

    server.shutdown().await.expect("shutdown");
}
