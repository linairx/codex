use super::*;
use pretty_assertions::assert_eq;

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
