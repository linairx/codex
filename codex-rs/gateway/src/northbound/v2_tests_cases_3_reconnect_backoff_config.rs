use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_does_not_fallback_config_read_by_cwd_during_reconnect_backoff() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-config".to_string(),
        project_id: Some("project-config".to_string()),
    };
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_a,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_b,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
        ],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the matching worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs_async(async {
        let err = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(serde_json::json!({
                    "includeLayers": true,
                    "cwd": "/tmp/worker-b/subdir",
                })),
                trace: None,
            },
        )
        .await
        .expect_err("cwd-aware config/read should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "required worker routes are unavailable for config/read: [1]"
        );
        assert_eq!(router.worker_count(), 1);
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"config/read\""));
    assert!(logs.contains("tenant-config"));
    assert!(logs.contains("project-config"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "available_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "unavailable_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_routes=[(1, \"{worker_b_url}\", "
    )));
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_threadless_config_read_during_reconnect_backoff() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-b/config.toml",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-config".to_string(),
        project_id: Some("project-config".to_string()),
    };
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_a,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_b,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
        ],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    assert!(
        router.remove_worker(Some(0)),
        "test should drop the primary worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(0, Instant::now(), Duration::from_secs(60));

    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs_async(async {
        let err = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(serde_json::json!({
                    "includeLayers": true,
                    "cwd": null,
                })),
                trace: None,
            },
        )
        .await
        .expect_err("threadless config/read should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "primary worker route is unavailable for config/read"
        );
        assert_eq!(router.worker_count(), 1);
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"config/read\""));
    assert!(logs.contains("tenant-config"));
    assert!(logs.contains("project-config"));
    assert!(logs.contains("available_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "available_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("unavailable_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "unavailable_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_routes=[(0, \"{worker_a_url}\", "
    )));
}
