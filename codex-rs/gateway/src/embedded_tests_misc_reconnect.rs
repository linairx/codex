use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_runtime_reconnects_workers_after_disconnect() {
    let worker_a = start_reconnecting_mock_remote_server(Some("secret-token".to_string())).await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker_a.clone(),
                    auth_token: Some("secret-token".to_string()),
                    account_id: None,
                }],
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
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first_thread: ThreadResponse = first_response.json().await.expect("first thread");
    assert_eq!(first_thread.thread.id, "thread-worker-a-1");

    let second_response = timeout(Duration::from_secs(5), async {
        loop {
            sleep(Duration::from_millis(100)).await;
            let response = client
                .post(format!("http://{}/v1/threads", server.local_addr()))
                .json(&CreateThreadRequest {
                    cwd: Some("/tmp/project-b".to_string()),
                    model: None,
                    ephemeral: Some(true),
                })
                .send()
                .await
                .expect("second response");
            if response.status() == reqwest::StatusCode::OK {
                break response;
            }
        }
    })
    .await
    .expect("worker should reconnect");
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(second_thread.thread.id, "thread-worker-a-2");

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.status, GatewayHealthStatus::Ok);
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers.len(), 1);
    assert_eq!(remote_workers[0].websocket_url, worker_a);
    assert_eq!(remote_workers[0].healthy, true);
    assert_eq!(remote_workers[0].reconnecting, false);
    assert_eq!(remote_workers[0].reconnect_attempt_count, 0);
    assert_eq!(
        remote_workers[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("remote app server")),
        true
    );
    assert_eq!(remote_workers[0].next_reconnect_at, None);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_streams_worker_reconnect_events_over_sse() {
    let worker_a = start_reconnecting_mock_remote_server(Some("secret-token".to_string())).await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker_a.clone(),
                    auth_token: Some("secret-token".to_string()),
                    account_id: None,
                }],
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

    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);

    let (reconnecting_event, reconnected_event) = timeout(Duration::from_secs(5), async {
        let mut buffered = String::new();
        let mut reconnecting_event = None;
        let mut reconnected_event = None;
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

                if event.contains("event: gateway/reconnecting") {
                    reconnecting_event = Some(event);
                } else if event.contains("event: gateway/reconnected") {
                    reconnected_event = Some(event);
                }

                if let (Some(reconnecting_event), Some(reconnected_event)) =
                    (reconnecting_event.as_ref(), reconnected_event.as_ref())
                {
                    return (reconnecting_event.clone(), reconnected_event.clone());
                }
            }
        }
    })
    .await
    .expect("timed out waiting for worker reconnect events");

    assert_eq!(
        reconnecting_event.contains("event: gateway/reconnecting"),
        true
    );
    assert_eq!(reconnecting_event.contains("\"workerId\":"), true);
    assert_eq!(reconnecting_event.contains(&worker_a), true);
    assert_eq!(reconnecting_event.contains("\"retryDelaySeconds\":0"), true);
    assert_eq!(
        reconnecting_event.contains("\"reason\":\"remote app server"),
        true
    );

    assert_eq!(
        reconnected_event.contains("event: gateway/reconnected"),
        true
    );
    assert_eq!(reconnected_event.contains("\"workerId\":"), true);
    assert_eq!(reconnected_event.contains(&worker_a), true);

    let second_response = timeout(Duration::from_secs(5), async {
        loop {
            sleep(Duration::from_millis(100)).await;
            let response = client
                .post(format!("http://{}/v1/threads", server.local_addr()))
                .json(&CreateThreadRequest {
                    cwd: Some("/tmp/project-b".to_string()),
                    model: None,
                    ephemeral: Some(true),
                })
                .send()
                .await
                .expect("second response");
            if response.status() == reqwest::StatusCode::OK {
                break response;
            }
        }
    })
    .await
    .expect("worker should reconnect");
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(second_thread.thread.id, "thread-worker-a-2");

    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_runtime_streams_worker_reconnect_events_over_sse() {
    let worker_a = start_reconnecting_mock_remote_server(Some("secret-token".to_string())).await;
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
                        websocket_url: worker_a.clone(),

                        auth_token: Some("secret-token".to_string()),
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b.clone(),

                        auth_token: Some("secret-token".to_string()),
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

    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first_thread: ThreadResponse = first_response.json().await.expect("first thread");
    assert_eq!(first_thread.thread.id, "thread-worker-a-1");

    let (reconnecting_event, reconnected_event) = timeout(Duration::from_secs(5), async {
        let mut buffered = String::new();
        let mut reconnecting_event = None;
        let mut reconnected_event = None;
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

                if event.contains("event: gateway/reconnecting") && event.contains(&worker_a) {
                    reconnecting_event = Some(event);
                } else if event.contains("event: gateway/reconnected") && event.contains(&worker_a)
                {
                    reconnected_event = Some(event);
                }

                if let (Some(reconnecting_event), Some(reconnected_event)) =
                    (reconnecting_event.as_ref(), reconnected_event.as_ref())
                {
                    return (reconnecting_event.clone(), reconnected_event.clone());
                }
            }
        }
    })
    .await
    .expect("timed out waiting for worker reconnect events");

    assert_eq!(
        reconnecting_event.contains("event: gateway/reconnecting"),
        true
    );
    assert_eq!(reconnecting_event.contains("\"workerId\":"), true);
    assert_eq!(reconnecting_event.contains(&worker_a), true);
    assert_eq!(
        reconnecting_event.contains("\"reason\":\"remote app server"),
        true
    );

    assert_eq!(
        reconnected_event.contains("event: gateway/reconnected"),
        true
    );
    assert_eq!(reconnected_event.contains("\"workerId\":"), true);
    assert_eq!(reconnected_event.contains(&worker_a), true);

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-b".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("second response");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(second_thread.thread.id, "thread-worker-b");

    let settled_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("expected remote worker health");
            };
            if remote_workers[0].healthy && !remote_workers[0].reconnecting {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("worker health should settle after reconnect");
    let remote_workers = settled_health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers[0].websocket_url, worker_a);
    assert_eq!(remote_workers[0].healthy, true);
    assert_eq!(remote_workers[0].reconnecting, false);

    let third_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-c".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("third response");
    assert_eq!(third_response.status(), reqwest::StatusCode::OK);
    let third_thread: ThreadResponse = third_response.json().await.expect("third thread");
    assert_eq!(third_thread.thread.id, "thread-worker-a-2");

    drop(events_response);
    server.shutdown().await.expect("shutdown");
}
