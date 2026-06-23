use super::*;
use pretty_assertions::assert_eq;

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
