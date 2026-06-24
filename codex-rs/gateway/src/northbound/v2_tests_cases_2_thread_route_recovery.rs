use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_request_routing_handoff::recover_visible_thread_worker_route;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_probes_visible_thread_read_across_workers_when_route_missing() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async(async {
        let thread_read_params = serde_json::json!({
            "threadId": "thread-worker-b",
            "includeTurns": false,
        });
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params.clone(),
            JSONRPCErrorError {
                code: INVALID_PARAMS_CODE,
                message: "thread not found: thread-worker-b".to_string(),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_thread_list_and_read(
            "thread-worker-b",
            "Worker B thread",
            "/tmp/worker-b",
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-visible".to_string(),
            project_id: Some("project-visible".to_string()),
        };
        scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
        let admission = GatewayAdmissionController::default();
        let observability = GatewayObservability::new(Some(metrics.clone()), false);
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

        let result = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("thread-read".to_string()),
                method: "thread/read".to_string(),
                params: Some(thread_read_params),
                trace: None,
            },
        )
        .await
        .expect("thread/read should reach downstream workers")
        .expect("thread/read should succeed through probed worker");

        assert_eq!(
            result,
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Worker B thread",
                    "cwd": "/tmp/worker-b",
                },
            })
        );
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    })
    .await;

    assert!(
        logs.contains("recovered missing visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread-worker-b"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert_v2_thread_route_recovery_metric(&metrics, "success");
    assert_no_v2_metric(&metrics, "gateway_v2_fail_closed_requests");
    assert_no_v2_metric(&metrics, "gateway_v2_upstream_request_failures");
}

#[tokio::test]
async fn visible_thread_route_recovery_fails_closed_during_reconnect_backoff() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "thread/read",
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Wrong worker thread",
                    "cwd": "/tmp/worker-a",
                },
            }),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "thread/read",
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Worker B thread",
                    "cwd": "/tmp/worker-b",
                },
            }),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
        "test should drop the owning worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

    let metrics = in_memory_metrics();
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
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

    let err = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .await
    .expect_err("visible thread route recovery should fail closed during reconnect backoff");

    assert_eq!(
        err.to_string(),
        "required worker routes are unavailable for thread/read: [1]"
    );
    assert_eq!(router.worker_count(), 1);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), None);
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    assert_v2_fail_closed_request_metric(&metrics, "thread/read", true);
}

#[tokio::test]
async fn recover_visible_thread_worker_route_logs_when_probe_finds_no_owner() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async(async {
        let thread_read_params = serde_json::json!({
            "threadId": "thread-missing",
            "includeTurns": false,
        });
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params.clone(),
            JSONRPCErrorError {
                code: INVALID_PARAMS_CODE,
                message: "thread not found: thread-missing".to_string(),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params,
            JSONRPCErrorError {
                code: INVALID_PARAMS_CODE,
                message: "thread not found: thread-missing".to_string(),
                data: None,
            },
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-visible".to_string(),
            project_id: Some("project-visible".to_string()),
        };
        scope_registry.register_thread("thread-missing".to_string(), context.clone());

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
        let router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");

        recover_visible_thread_worker_route(
            &router,
            &scope_registry,
            &context,
            &GatewayObservability::new(Some(metrics.clone()), false),
            "thread-missing",
        )
        .await
        .expect("thread route probe should complete");

        assert_eq!(scope_registry.thread_worker_id("thread-missing"), None);
    })
    .await;

    assert!(
        logs.contains("failed to recover visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread-missing"));
    assert!(logs.contains("attempted_worker_ids=[Some(0), Some(1)]"));
    assert!(logs.contains("attempted_worker_websocket_urls=["));
    assert_v2_thread_route_recovery_metric(&metrics, "miss");
    assert_no_v2_metric(&metrics, "gateway_v2_fail_closed_requests");
    assert_no_v2_metric(&metrics, "gateway_v2_upstream_request_failures");
}

#[tokio::test]
async fn handle_client_request_recovers_visible_thread_route_before_resume() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/read",
        serde_json::json!({
            "threadId": "thread-worker-b",
            "includeTurns": false,
        }),
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_thread_list_and_read(
        "thread-worker-b",
        "Worker B thread",
        "/tmp/worker-b",
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
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

    let result = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume".to_string()),
            method: "thread/resume".to_string(),
            params: Some(thread_resume_params),
            trace: None,
        },
    )
    .await
    .expect("thread/resume should reach downstream workers")
    .expect("thread/resume should succeed through recovered worker route");

    assert_eq!(
        result,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}
