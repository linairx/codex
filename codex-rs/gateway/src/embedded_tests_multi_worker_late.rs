use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_multi_worker_late_thread_routing.rs"]
mod embedded_tests_multi_worker_late_thread_routing;

#[tokio::test]
async fn remote_runtime_same_project_routes_around_unhealthy_affinity_worker() {
    let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token: Some("secret-token".to_string()),
        thread_id: "thread-worker-a",
        preview: "/tmp/worker-a",
        close_after_first_request: true,
    })
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
    let event_task = tokio::spawn({
        let event_url = event_url.clone();
        async move {
            let mut events_response = event_client
                .get(event_url)
                .header("x-codex-project-id", "project-a")
                .send()
                .await
                .expect("event stream response");
            assert_eq!(events_response.status(), reqwest::StatusCode::OK);

            let mut buffered = String::new();
            let mut first_route_event = None;
            let mut second_route_event = None;
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

                    if event.contains("event: gateway/projectWorkerRouteSelected")
                        && event.contains("\"threadId\":\"thread-worker-a\"")
                    {
                        first_route_event = Some(event.clone());
                    } else if event.contains("event: gateway/projectWorkerRouteSelected")
                        && event.contains("\"threadId\":\"thread-worker-b\"")
                    {
                        second_route_event = Some(event.clone());
                    }

                    if let (Some(first_route_event), Some(second_route_event)) =
                        (first_route_event.as_ref(), second_route_event.as_ref())
                    {
                        return (first_route_event.clone(), second_route_event.clone());
                    }
                }
            }
        }
    });
    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
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
    assert_eq!(first_thread.thread.id, "thread-worker-a");

    sleep(Duration::from_millis(100)).await;

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("second response");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
    assert_eq!(second_thread.thread.id, "thread-worker-b");

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
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
    assert_eq!(
        health.v2_connections.project_worker_route_selection_count,
        2
    );
    assert_eq!(
        health
            .v2_connections
            .project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 0,
                project_worker_route_selection_count: 1,
            },
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 1,
                project_worker_route_selection_count: 1,
            },
        ]
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_worker_id,
        Some(1)
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("default")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-worker-b")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_account_id
            .as_deref(),
        Some("acct-b")
    );
    assert!(
        health
            .v2_connections
            .last_project_worker_route_selected_at
            .is_some()
    );

    let (first_route_event, second_route_event) = timeout(Duration::from_secs(5), event_task)
        .await
        .expect("timed out waiting for project route events")
        .expect("event task should finish");
    assert_eq!(
        first_route_event.contains("event: gateway/projectWorkerRouteSelected"),
        true
    );
    assert_eq!(first_route_event.contains("\"tenantId\":\"default\""), true);
    assert_eq!(
        first_route_event.contains("\"projectId\":\"project-a\""),
        true
    );
    assert_eq!(
        first_route_event.contains("\"threadId\":\"thread-worker-a\""),
        true
    );
    assert_eq!(first_route_event.contains("\"workerId\":0"), true);
    assert_eq!(first_route_event.contains("\"accountId\":\"acct-a\""), true);
    assert_eq!(
        second_route_event.contains("event: gateway/projectWorkerRouteSelected"),
        true
    );
    assert_eq!(
        second_route_event.contains("\"threadId\":\"thread-worker-b\""),
        true
    );
    assert_eq!(second_route_event.contains("\"workerId\":1"), true);
    assert_eq!(
        second_route_event.contains("\"accountId\":\"acct-b\""),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_reports_mixed_project_route_eligibility_after_worker_health_changes() {
    let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token: Some("secret-token".to_string()),
        thread_id: "thread-worker-a",
        preview: "/tmp/worker-a",
        close_after_first_request: true,
    })
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
    let project_a_event_task = tokio::spawn({
        let event_url = event_url.clone();
        async move {
            let mut events_response = event_client
                .get(event_url)
                .header("x-codex-project-id", "project-a")
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

                    if event.contains("event: gateway/projectWorkerRouteSelected")
                        && event.contains("\"threadId\":\"thread-worker-a\"")
                    {
                        return event;
                    }
                }
            }
        }
    });
    let project_b_event_task = tokio::spawn(async move {
        let mut events_response = reqwest::Client::new()
            .get(event_url)
            .header("x-codex-project-id", "project-b")
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

                if event.contains("event: gateway/projectWorkerRouteSelected")
                    && event.contains("\"threadId\":\"thread-worker-b\"")
                {
                    return event;
                }
            }
        }
    });

    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
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
    assert_eq!(first_thread.thread.id, "thread-worker-a");

    sleep(Duration::from_millis(100)).await;

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-b")
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

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    let remote_workers = health.remote_workers.as_ref().expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(remote_workers[0].account_id.as_deref(), Some("acct-a"));
    assert_eq!(remote_workers[0].healthy, false);
    assert_eq!(remote_workers[0].reconnecting, true);
    assert_eq!(remote_workers[1].account_id.as_deref(), Some("acct-b"));
    assert_eq!(remote_workers[1].healthy, true);
    assert_eq!(remote_workers[1].reconnecting, false);
    assert_eq!(
        health.project_worker_routes,
        Some(vec![
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "default".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: false,
                account_routing_eligible: false,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "default".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
        ])
    );

    let first_route_event = timeout(Duration::from_secs(5), project_a_event_task)
        .await
        .expect("timed out waiting for project-a route event")
        .expect("project-a event task should finish");
    assert!(first_route_event.contains("event: gateway/projectWorkerRouteSelected"));
    assert!(first_route_event.contains("\"tenantId\":\"default\""));
    assert!(first_route_event.contains("\"projectId\":\"project-a\""));
    assert!(first_route_event.contains("\"threadId\":\"thread-worker-a\""));
    assert!(first_route_event.contains("\"workerId\":0"));
    assert!(first_route_event.contains("\"accountId\":\"acct-a\""));

    let second_route_event = timeout(Duration::from_secs(5), project_b_event_task)
        .await
        .expect("timed out waiting for project-b route event")
        .expect("project-b event task should finish");
    assert!(second_route_event.contains("event: gateway/projectWorkerRouteSelected"));
    assert!(second_route_event.contains("\"tenantId\":\"default\""));
    assert!(second_route_event.contains("\"projectId\":\"project-b\""));
    assert!(second_route_event.contains("\"threadId\":\"thread-worker-b\""));
    assert!(second_route_event.contains("\"workerId\":1"));
    assert!(second_route_event.contains("\"accountId\":\"acct-b\""));

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_falls_back_to_unlabeled_project_route_after_labeled_worker_disconnects()
 {
    let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token: Some("secret-token".to_string()),
        thread_id: "thread-worker-a",
        preview: "/tmp/worker-a",
        close_after_first_request: true,
    })
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

    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
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
    assert_eq!(first_thread.thread.id, "thread-worker-a");

    let event_url = format!("http://{}/v1/events", server.local_addr());
    let unlabeled_route_event_task = tokio::spawn({
        let event_url = event_url.clone();
        async move {
            let mut events_response = reqwest::Client::new()
                .get(event_url)
                .header("x-codex-project-id", "project-b")
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

                    if event.contains("event: gateway/projectWorkerRouteSelected")
                        && event.contains("\"threadId\":\"thread-worker-b\"")
                        && event.contains("\"accountId\":null")
                    {
                        return event;
                    }
                }
            }
        }
    });

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-b")
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

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.remote_account_labels_complete, Some(false));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(1));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(vec![1]));
    assert_eq!(
        health.remote_unlabeled_account_workers,
        Some(vec![crate::api::GatewayRemoteUnlabeledAccountWorker {
            worker_id: 1,
            websocket_url: worker_b,
        }])
    );
    assert_eq!(
        health.project_worker_routes,
        Some(vec![
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "default".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: false,
                account_routing_eligible: false,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "default".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 1,
                account_id: None,
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: false,
            },
        ])
    );
    assert_eq!(
        health.v2_connections.project_worker_route_selection_count,
        2
    );
    assert_eq!(
        health
            .v2_connections
            .project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 0,
                project_worker_route_selection_count: 1,
            },
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 1,
                project_worker_route_selection_count: 1,
            },
        ]
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_worker_id,
        Some(1)
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("default")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-b")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-worker-b")
    );
    assert_eq!(
        health
            .v2_connections
            .last_project_worker_route_selected_account_id
            .as_deref(),
        None
    );

    let unlabeled_route_event = timeout(Duration::from_secs(5), unlabeled_route_event_task)
        .await
        .expect("timed out waiting for unlabeled project route event")
        .expect("unlabeled route event task should finish");
    assert!(unlabeled_route_event.contains("event: gateway/projectWorkerRouteSelected"));
    assert!(unlabeled_route_event.contains("\"tenantId\":\"default\""));
    assert!(unlabeled_route_event.contains("\"projectId\":\"project-b\""));
    assert!(unlabeled_route_event.contains("\"threadId\":\"thread-worker-b\""));
    assert!(unlabeled_route_event.contains("\"workerId\":1"));
    assert!(unlabeled_route_event.contains("\"accountId\":null"));

    server.shutdown().await.expect("shutdown");
}

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
                } else if event.contains("event: gateway/accountFailoverSucceeded") {
                    failover_event = Some(event);
                }

                if let (Some(exhausted_event), Some(failover_event)) =
                    (exhausted_event.as_ref(), failover_event.as_ref())
                {
                    return (exhausted_event.clone(), failover_event.clone());
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

    let (exhausted_event, failover_event) = timeout(Duration::from_secs(5), event_task)
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

#[tokio::test]
async fn remote_runtime_returns_bad_gateway_for_threads_on_unhealthy_workers() {
    let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token: Some("secret-token".to_string()),
        thread_id: "thread-worker-a",
        preview: "/tmp/worker-a",
        close_after_first_request: true,
    })
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
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: worker_a,
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
    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("create response");
    assert_eq!(create_response.status(), reqwest::StatusCode::OK);
    let thread: ThreadResponse = create_response.json().await.expect("thread");

    sleep(Duration::from_millis(100)).await;

    let read_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            thread.thread.id
        ))
        .send()
        .await
        .expect("read response");
    assert_eq!(read_response.status(), reqwest::StatusCode::BAD_GATEWAY);
    assert_eq!(
        read_response
            .text()
            .await
            .expect("body")
            .contains("unhealthy"),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_healthz_reports_degraded_workers() {
    let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
        expected_auth_token: Some("secret-token".to_string()),
        thread_id: "thread-worker-a",
        preview: "/tmp/worker-a",
        close_after_first_request: true,
    })
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
    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-project-id", "project-a")
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("create response");
    assert_eq!(create_response.status(), reqwest::StatusCode::OK);

    sleep(Duration::from_millis(100)).await;

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.status, GatewayHealthStatus::Degraded);
    assert_eq!(health.runtime_mode, "remote");
    assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(remote_workers[0].worker_id, 0);
    assert_eq!(remote_workers[0].websocket_url, worker_a);
    assert_eq!(remote_workers[0].healthy, false);
    assert_eq!(remote_workers[0].reconnecting, true);
    assert_eq!(remote_workers[0].reconnect_attempt_count, 0);
    assert_eq!(
        remote_workers[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("remote app server")),
        true
    );
    assert_eq!(remote_workers[0].next_reconnect_at.is_some(), true);
    assert_eq!(
        remote_workers[0]
            .reconnect_backoff_remaining_seconds
            .is_some_and(|remaining_seconds| remaining_seconds >= 0),
        true
    );
    assert_eq!(remote_workers[1].worker_id, 1);
    assert_eq!(remote_workers[1].websocket_url, worker_b);
    assert_eq!(remote_workers[1].healthy, true);
    assert_eq!(remote_workers[1].reconnecting, false);
    assert_eq!(remote_workers[1].reconnect_attempt_count, 0);
    assert_eq!(remote_workers[1].last_error, None);
    assert_eq!(remote_workers[1].next_reconnect_at, None);
    assert_eq!(remote_workers[1].reconnect_backoff_remaining_seconds, None);
    assert_eq!(
        health.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "default".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 0,
            account_id: None,
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: false,
            account_routing_eligible: false,
        }])
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_runtime_healthz_reports_v2_as_supported() {
    let worker_a = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-worker-a",
        "/tmp/worker-a",
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
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

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
    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.status, GatewayHealthStatus::Ok);
    assert_eq!(health.runtime_mode, "remote");
    assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(health.remote_workers.as_ref().map(Vec::len), Some(2));

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_runtime_accepts_v2_websocket_upgrades() {
    let worker_a = start_mock_remote_server(
        Some("secret-token".to_string()),
        "thread-worker-a",
        "/tmp/worker-a",
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
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

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

    let request = format!("ws://{}/", server.local_addr())
        .into_client_request()
        .expect("request should build");
    let (mut websocket, response) = connect_async(request)
        .await
        .expect("websocket upgrade should succeed for remote multi-worker v2");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);
    websocket.close(None).await.expect("websocket should close");

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_client_thread_routing_and_aggregation() {
    let worker_a =
        start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a").await;
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
        channel_capacity: 16,
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
        .expect("first thread/start should succeed through multi-worker remote gateway");
    assert_eq!(first_started.thread.id, "thread-worker-a");

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
        .expect("second thread/start should succeed through multi-worker remote gateway");
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(3),
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
        .expect("thread/list should succeed through multi-worker remote gateway");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a"]
    );

    let loaded: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(4),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should succeed through multi-worker remote gateway");
    assert_eq!(loaded.next_cursor, None);
    assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);

    let first_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to worker A");
    assert_eq!(first_read.thread.id, first_started.thread.id);
    assert_eq!(
        first_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );

    let second_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: second_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to worker B");
    assert_eq!(second_read.thread.id, second_started.thread.id);
    assert_eq!(
        second_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b"
    );

    let first_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(7),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(first_started.thread.id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to worker A");
    assert_eq!(first_apps.next_cursor, None);
    assert_eq!(
        first_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-app"]
    );

    let second_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(8),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(second_started.thread.id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to worker B");
    assert_eq!(second_apps.next_cursor, None);
    assert_eq!(
        second_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b-app"]
    );

    let first_mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(9),
            params: McpResourceReadParams {
                thread_id: Some(first_started.thread.id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-a/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to worker A");
    assert_eq!(
        serde_json::to_value(&first_mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-a/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-a resource",
        }])
    );

    let second_mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(10),
            params: McpResourceReadParams {
                thread_id: Some(second_started.thread.id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-b/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to worker B");
    assert_eq!(
        serde_json::to_value(&second_mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-b/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-b resource",
        }])
    );

    let first_mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(11),
            params: McpServerToolCallParams {
                thread_id: first_started.thread.id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-a" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to worker A");
    assert_eq!(
        first_mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-a" }))
    );
    assert_eq!(first_mcp_tool.is_error, Some(false));

    let second_mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(12),
            params: McpServerToolCallParams {
                thread_id: second_started.thread.id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-b" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to worker B");
    assert_eq!(
        second_mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-b" }))
    );
    assert_eq!(second_mcp_tool.is_error, Some(false));

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
