use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_runtime_fails_thread_read_handoff_when_replacement_returns_wrong_thread() {
    let worker_a = start_mock_remote_multi_connection_wrong_thread_read_server().await;
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

                if event.contains("event: gateway/accountThreadHandoffFailed") {
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
    assert_eq!(read_response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let error_body = read_response.text().await.expect("error body");
    assert!(
        error_body.contains(
            "replacement worker returned thread 00000000-0000-0000-0000-0000000000a1 while restoring 00000000-0000-0000-0000-0000000000b2"
        ),
        "{error_body}"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for thread/read handoff failure event")
        .expect("event task should finish");
    assert!(handoff_event.contains("event: gateway/accountThreadHandoffFailed"));
    assert!(handoff_event.contains("\"method\":\"thread/read\""));
    assert!(handoff_event.contains("\"threadId\":\"00000000-0000-0000-0000-0000000000b2\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":1"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-b\""));
    assert!(handoff_event.contains("replacement worker returned thread"));

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_fails_closed_when_server_request_response_targets_exhausted_account() {
    let worker_a = start_mock_remote_server_for_server_request_roundtrip().await;
    let worker_b = start_mock_remote_server_for_thread_start_error(
        Some("secret-token".to_string()),
        JSONRPCErrorError {
            code: 429,
            message: "rate limit reached".to_string(),
            data: None,
        },
    )
    .await;
    let worker_c = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-worker-c",
        "/tmp/worker-c",
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

                        auth_token: Some("secret-token".to_string()),
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_c,

                        auth_token: Some("secret-token".to_string()),
                        account_id: Some("acct-c".to_string()),
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
    let request_event_client = client.clone();
    let request_event_url = format!("http://{}/v1/events", server.local_addr());
    let request_event_task = tokio::spawn(async move {
        let mut events_response = request_event_client
            .get(request_event_url)
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

                if event.contains("event: serverRequest/requested")
                    && event.contains("\"requestId\":\"srv-user-input\"")
                {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/remote-project".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let thread: ThreadResponse = response.json().await.expect("thread");
    assert_eq!(thread.thread.id, "thread-remote-workflow");

    let requested_event = timeout(Duration::from_secs(5), request_event_task)
        .await
        .expect("timed out waiting for pending server request")
        .expect("event task should finish");
    assert!(requested_event.contains("\"threadId\":\"thread-remote-workflow\""));

    let failover_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/worker-c".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("failover thread response");
    assert_eq!(failover_response.status(), reqwest::StatusCode::OK);
    let failover_thread: ThreadResponse = failover_response.json().await.expect("failover thread");
    assert_eq!(failover_thread.thread.id, "thread-worker-c");

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

                if event.contains("event: gateway/accountActiveThreadHandoffFailed") {
                    return event;
                }
            }
        }
    });
    sleep(Duration::from_millis(100)).await;

    let response = client
        .post(format!(
            "http://{}/v1/server-requests/respond",
            server.local_addr()
        ))
        .json(&serde_json::json!({
            "requestId": "srv-user-input",
            "type": "toolRequestUserInput",
            "answers": {
                "mode": {
                    "answers": ["safe"],
                },
            },
        }))
        .send()
        .await
        .expect("server request response");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let error_body = response.text().await.expect("error body");
    assert!(
        error_body.contains(
            "thread thread-remote-workflow is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        ),
        "{error_body}"
    );

    let handoff_event = timeout(Duration::from_secs(5), handoff_event_task)
        .await
        .expect("timed out waiting for active thread handoff failure event")
        .expect("event task should finish");
    assert!(handoff_event.contains("event: gateway/accountActiveThreadHandoffFailed"));
    assert!(handoff_event.contains("\"method\":\"serverRequest/respond\""));
    assert!(handoff_event.contains("\"threadId\":\"thread-remote-workflow\""));
    assert!(handoff_event.contains("\"exhaustedWorkerId\":0"));
    assert!(handoff_event.contains("\"exhaustedAccountId\":\"acct-a\""));

    server.shutdown().await.expect("shutdown");
}
