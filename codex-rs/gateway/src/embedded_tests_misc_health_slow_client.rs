use super::super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn remote_single_worker_healthz_reports_slow_client_timeout_outcome() {
    let worker = start_mock_remote_server_for_slow_client_timeout().await;
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
                    websocket_url: worker,
                    auth_token: None,
                    account_id: None,
                }],
            }),
            v2_client_send_timeout: Duration::from_millis(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let health_client = reqwest::Client::new();
    let mut request = format!("ws://{}/", server.local_addr())
        .into_client_request()
        .expect("request should build");
    request.headers_mut().insert(
        "x-codex-tenant-id",
        "tenant-visible".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-visible".parse().expect("project header"),
    );
    let (mut websocket, response) = connect_async(request)
        .await
        .expect("websocket should connect");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);

    write_websocket_message(
        &mut websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "clientInfo": {
                    "name": "codex-gateway-test",
                    "version": "0.0.0-test",
                },
                "capabilities": null,
            })),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Response(_) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize response");
    };
    let JSONRPCMessage::Request(server_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded server request before slow-client timeout");
    };
    assert_eq!(server_request.method, "account/chatgptAuthTokens/refresh");

    let active_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health
                .v2_connections
                .active_connection_server_request_backlog_method_counts
                == vec![GatewayV2ServerRequestBacklogMethodCounts {
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                }]
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("active server-request backlog method health should settle");
    assert_eq!(active_health.v2_connections.active_connection_count, 1);
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_peak_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(0),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );

    let settled_health = timeout(Duration::from_secs(60), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health.v2_connections.last_connection_outcome.is_some()
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("connection health should settle");
    assert_eq!(settled_health.runtime_mode, "remote");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(
        settled_health.v2_connections.connection_outcome_counts,
        vec![GatewayV2ConnectionOutcomeCounts {
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(settled_health.v2_connections.client_send_timeout_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_client_send_timeout_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_method
            .as_deref(),
        Some("warning")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_outcome
            .as_deref(),
        Some("client_send_timed_out")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.peak_active_connection_count,
        1
    );
    assert_eq!(settled_health.v2_connections.total_connection_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_detail,
        Some("gateway websocket send timed out".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_duration_ms
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.max_connection_duration_ms,
        settled_health.v2_connections.last_connection_duration_ms
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "account/chatgptAuthTokens/refresh".to_string(),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(0),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_healthz_reports_slow_client_timeout_outcome() {
    let worker_a = start_mock_remote_server_for_idle_v2_sessions(None).await;
    let worker_b = start_mock_remote_server_for_slow_client_timeout().await;
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
            v2_client_send_timeout: Duration::from_millis(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let health_client = reqwest::Client::new();
    let mut request = format!("ws://{}/", server.local_addr())
        .into_client_request()
        .expect("request should build");
    request.headers_mut().insert(
        "x-codex-tenant-id",
        "tenant-visible".parse().expect("tenant header"),
    );
    request.headers_mut().insert(
        "x-codex-project-id",
        "project-visible".parse().expect("project header"),
    );
    let (mut websocket, response) = connect_async(request)
        .await
        .expect("websocket should connect");
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);

    write_websocket_message(
        &mut websocket,
        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "clientInfo": {
                    "name": "codex-gateway-test",
                    "version": "0.0.0-test",
                },
                "capabilities": null,
            })),
            trace: None,
        }),
    )
    .await;

    let JSONRPCMessage::Response(_) = read_websocket_message(&mut websocket).await else {
        panic!("expected initialize response");
    };
    let JSONRPCMessage::Request(server_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded server request before slow-client timeout");
    };
    assert_eq!(server_request.method, "account/chatgptAuthTokens/refresh");

    let active_health = timeout(Duration::from_secs(5), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health
                .v2_connections
                .active_connection_server_request_backlog_method_counts
                == vec![GatewayV2ServerRequestBacklogMethodCounts {
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                }]
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("active server-request backlog method health should settle");
    assert_eq!(active_health.v2_connections.active_connection_count, 1);
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_peak_server_request_backlog_count,
        1
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        active_health
            .v2_connections
            .active_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(1),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );

    let settled_health = timeout(Duration::from_secs(60), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health.v2_connections.last_connection_outcome.is_some()
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("connection health should settle");
    assert_eq!(settled_health.runtime_mode, "remote");
    assert_eq!(
        settled_health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(settled_health.v2_connections.client_send_timeout_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_client_send_timeout_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_method
            .as_deref(),
        Some("warning")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_outcome
            .as_deref(),
        Some("client_send_timed_out")
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_notification_send_failure_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.peak_active_connection_count,
        1
    );
    assert_eq!(settled_health.v2_connections.total_connection_count, 1);
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.v2_connections.last_connection_detail,
        Some("gateway websocket send timed out".to_string())
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        1
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "account/chatgptAuthTokens/refresh".to_string(),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![GatewayV2ServerRequestBacklogWorkerCounts {
            worker_id: Some(1),
            pending_server_request_count: 1,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 1,
        }]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_completed_at
            .is_some(),
        true
    );

    server.shutdown().await.expect("shutdown");
}
