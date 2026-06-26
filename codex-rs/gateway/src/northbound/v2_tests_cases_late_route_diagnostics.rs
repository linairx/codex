use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
pub(crate) async fn log_fail_closed_multi_worker_request_includes_worker_route_details() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://worker-a.invalid".to_string(),
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
                            websocket_url: "ws://worker-b.invalid".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(30));

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
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs(|| {
        super::super::super::super::log_fail_closed_multi_worker_request(
            &router,
            &connection,
            "config/read",
            &std::io::Error::other(
                super::super::super::super::FailClosedMultiWorkerRouteError {
                    message: "required worker routes are unavailable for config/read: [1]"
                        .to_string(),
                },
            ),
        );
    });

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("config/read"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains("available_worker_websocket_urls=[\"ws://worker-a.invalid\"]"));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains("unavailable_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
    assert!(logs.contains("reconnect_backoff_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains("reconnect_backoff_worker_routes=[(1, \"ws://worker-b.invalid\", "));
    assert_v2_fail_closed_request_metric(&metrics, "config/read", true);

    router.clear_worker_reconnect_failure(1);
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs(|| {
        super::super::super::super::log_fail_closed_multi_worker_request(
            &router,
            &connection,
            "config/read",
            &std::io::Error::other(
                super::super::super::super::FailClosedMultiWorkerRouteError {
                    message: "required worker routes are unavailable for config/read: [1]"
                        .to_string(),
                },
            ),
        );
    });

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("config/read"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains("available_worker_websocket_urls=[\"ws://worker-a.invalid\"]"));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains("unavailable_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_ids=[]"));
    assert!(logs.contains("reconnect_backoff_worker_websocket_urls=[]"));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=[]"));
    assert!(logs.contains("reconnect_backoff_worker_routes=[]"));
    assert_v2_fail_closed_request_metric(&metrics, "config/read", false);

    let (capacity_client_a, capacity_request_handle_a) = start_test_request_handle().await;
    let (capacity_client_b, capacity_request_handle_b) = start_test_request_handle().await;
    let (capacity_event_tx, capacity_event_rx) = mpsc::channel(1);
    let capacity_router = GatewayV2DownstreamRouter {
        workers: vec![
            DownstreamWorkerHandle {
                worker_id: Some(0),
                worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
                request_handle: capacity_request_handle_a,
            },
            DownstreamWorkerHandle {
                worker_id: Some(1),
                worker_websocket_url: Some("ws://worker-b.invalid".to_string()),
                request_handle: capacity_request_handle_b,
            },
        ],
        event_tx: capacity_event_tx,
        event_rx: capacity_event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://worker-a.invalid".to_string(),
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
                            websocket_url: "ws://worker-b.invalid".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs(|| {
        super::super::super::super::log_fail_closed_multi_worker_request(
                &capacity_router,
                &connection,
                "turn/start",
                &std::io::Error::other(super::super::super::super::FailClosedMultiWorkerRouteError {
                    message: "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for turn/start"
                        .to_string(),
                }),
            );
    });

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("turn/start"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("available_worker_ids=[0, 1]"));
    assert!(logs.contains("unavailable_worker_ids=[]"));
    assert!(logs.contains("reconnect_backoff_worker_ids=[]"));
    assert!(logs.contains("exhausted account capacity"));
    assert_v2_fail_closed_request_metric(&metrics, "turn/start", false);
    capacity_client_a
        .shutdown()
        .await
        .expect("capacity client A should shut down");
    capacity_client_b
        .shutdown()
        .await
        .expect("capacity client B should shut down");

    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs(|| {
        super::super::super::super::log_fail_closed_multi_worker_request(
            &router,
            &connection,
            "config/read",
            &std::io::Error::other("ordinary upstream error"),
        );
    });

    assert!(
        logs.contains("gateway v2 upstream request failed while worker routes are unavailable")
    );
    assert!(logs.contains("ordinary upstream error"));
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let mut fail_closed_metric_count = 0;
    let mut saw_upstream_request_failure_count = false;
    for metric in resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
    {
        match metric.name() {
            "gateway_v2_fail_closed_requests" => {
                fail_closed_metric_count += 1;
            }
            "gateway_v2_upstream_request_failures" => {
                saw_upstream_request_failure_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([
                                    ("method".to_string(), "config/read".to_string()),
                                    ("reconnect_backoff_active".to_string(), "false".to_string(),),
                                ])
                            );
                        }
                        _ => panic!("unexpected upstream request failure count aggregation"),
                    },
                    _ => panic!("unexpected upstream request failure count type"),
                }
            }
            _ => {}
        }
    }
    assert_eq!(fail_closed_metric_count, 0);
    assert!(saw_upstream_request_failure_count);

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
pub(crate) async fn unavailable_worker_route_diagnostics_marks_backoff_workers() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let now = Instant::now();
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::from([(1, now + Duration::from_secs(30))]),
        reconnect_state: Some(super::super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://worker-a.invalid".to_string(),
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
                            websocket_url: "ws://worker-b.invalid".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };

    assert_eq!(
        router.unavailable_worker_route_diagnostics(now),
        vec![
            super::super::super::super::UnavailableWorkerRouteDiagnostics {
                worker_id: 1,
                websocket_url: "ws://worker-b.invalid".to_string(),
                reconnect_backoff_active: true,
                reconnect_backoff_remaining_seconds: Some(30),
            }
        ]
    );

    router.clear_worker_reconnect_failure(1);
    assert_eq!(
        router.unavailable_worker_route_diagnostics(now),
        vec![
            super::super::super::super::UnavailableWorkerRouteDiagnostics {
                worker_id: 1,
                websocket_url: "ws://worker-b.invalid".to_string(),
                reconnect_backoff_active: false,
                reconnect_backoff_remaining_seconds: None,
            }
        ]
    );

    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let logs = capture_logs(|| {
        log_degraded_multi_worker_thread_discovery(
            &router,
            &request_context,
            &observability,
            "thread/list",
        );
    });

    assert!(logs.contains("serving degraded multi-worker thread discovery from available workers"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains("available_worker_websocket_urls=[\"ws://worker-a.invalid\"]"));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains("unavailable_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_ids=[]"));
    assert!(logs.contains("reconnect_backoff_worker_websocket_urls=[]"));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=[]"));
    assert!(logs.contains("reconnect_backoff_worker_routes=[]"));
    assert_v2_degraded_thread_discovery_metric(&metrics, "thread/list", false);

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
pub(crate) async fn handle_client_request_records_unavailable_route_upstream_failure_metrics() {
    let (client, request_handle) = start_test_request_handle().await;
    client.shutdown().await.expect("client should shut down");

    let (event_tx, event_rx) = mpsc::channel(1);
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::from([(1, Instant::now() + Duration::from_secs(30))]),
        reconnect_state: Some(super::super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://worker-a.invalid".to_string(),
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
                            websocket_url: "ws://worker-b.invalid".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };

    let metrics = in_memory_metrics();
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let logs = capture_logs_async(async {
        let err = super::super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("config-requirements-read".to_string()),
                method: "configRequirements/read".to_string(),
                params: None,
                trace: None,
            },
        )
        .await
        .expect_err("closed downstream request handle should fail");

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    })
    .await;

    assert!(
        logs.contains("gateway v2 upstream request failed while worker routes are unavailable")
    );
    assert!(logs.contains("configRequirements/read"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains("available_worker_websocket_urls=[\"ws://worker-a.invalid\"]"));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains("unavailable_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
    assert!(logs.contains("reconnect_backoff_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains("reconnect_backoff_worker_routes=[(1, \"ws://worker-b.invalid\", "));
    assert_v2_upstream_request_failure_metric(&metrics, "configRequirements/read", true);
}
