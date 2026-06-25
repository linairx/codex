use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_late_notifications.rs"]
mod v2_tests_cases_late_notifications;

#[path = "v2_tests_cases_late_notification_dedupe.rs"]
mod v2_tests_cases_late_notification_dedupe;

#[path = "v2_tests_cases_late_route_diagnostics.rs"]
mod v2_tests_cases_late_route_diagnostics;

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
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
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
        super::super::super::log_fail_closed_multi_worker_request(
            &router,
            &connection,
            "config/read",
            &std::io::Error::other(super::super::super::FailClosedMultiWorkerRouteError {
                message: "required worker routes are unavailable for config/read: [1]".to_string(),
            }),
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
        super::super::super::log_fail_closed_multi_worker_request(
            &router,
            &connection,
            "config/read",
            &std::io::Error::other(super::super::super::FailClosedMultiWorkerRouteError {
                message: "required worker routes are unavailable for config/read: [1]".to_string(),
            }),
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
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
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
        super::super::super::log_fail_closed_multi_worker_request(
                &capacity_router,
                &connection,
                "turn/start",
                &std::io::Error::other(super::super::super::FailClosedMultiWorkerRouteError {
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
        super::super::super::log_fail_closed_multi_worker_request(
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

#[test]
pub(crate) fn log_worker_server_request_cleanup_includes_cleaned_up_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_worker_server_request_cleanup(
            Some(1),
            "ws://worker-b.invalid",
            1,
            Some("worker-b lost"),
            &super::super::super::WorkerServerRequestCleanup {
                resolved_notifications: vec![
                    super::super::super::WorkerCleanupResolvedNotification {
                        notification: ServerRequestResolvedNotification {
                            thread_id: "thread-visible".to_string(),
                            request_id: RequestId::String("gateway-thread-1".to_string()),
                        },
                        method: "item/tool/requestUserInput".to_string(),
                    },
                ],
                resolved_thread_scoped_requests: 1,
                resolved_thread_scoped_request_ids: vec![RequestId::String(
                    "gateway-thread-1".to_string(),
                )],
                resolved_thread_scoped_downstream_request_ids: vec![RequestId::String(
                    "downstream-thread-1".to_string(),
                )],
                resolved_thread_scoped_methods: vec!["item/tool/requestUserInput".to_string()],
                resolved_thread_scoped_thread_ids: vec!["thread-visible".to_string()],
                stranded_connection_scoped_requests: 1,
                stranded_connection_scoped_request_ids: vec![RequestId::String(
                    "gateway-connection-1".to_string(),
                )],
                stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
                    "downstream-connection-1".to_string(),
                )],
                stranded_connection_scoped_methods: vec![
                    "account/chatgptAuthTokens/refresh".to_string(),
                ],
            },
            "downstream worker disconnected within shared gateway v2 session",
        );
    });

    assert!(logs.contains("downstream worker disconnected within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("remaining_worker_count=1"));
    assert!(logs.contains("disconnect_message"));
    assert!(logs.contains("worker-b lost"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=1"));
    assert!(
        logs.contains("resolved_thread_scoped_server_request_ids=[String(\"gateway-thread-1\")]")
    );
    assert!(logs.contains(
        "resolved_thread_scoped_downstream_server_request_ids=[String(\"downstream-thread-1\")]"
    ));
    assert!(logs.contains(
        "resolved_thread_scoped_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("resolved_thread_scoped_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=1"));
    assert!(logs.contains(
        "stranded_connection_scoped_server_request_ids=[String(\"gateway-connection-1\")]"
    ));
    assert!(logs.contains(
            "stranded_connection_scoped_downstream_server_request_ids=[String(\"downstream-connection-1\")]"
        ));
    assert!(logs.contains(
        "stranded_connection_scoped_server_request_methods=[\"account/chatgptAuthTokens/refresh\"]"
    ));
}

#[test]
pub(crate) fn worker_server_request_cleanup_publishes_operator_event() {
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let observability = GatewayObservability::default().with_operator_events(operator_events_tx);

    super::super::super::publish_worker_server_request_cleanup_event(
        &observability,
        Some(1),
        "ws://worker-b.invalid",
        1,
        Some("worker-b lost"),
        &super::super::super::WorkerServerRequestCleanup {
            resolved_notifications: Vec::new(),
            resolved_thread_scoped_requests: 2,
            resolved_thread_scoped_request_ids: vec![
                RequestId::String("gateway-thread-2".to_string()),
                RequestId::String("gateway-thread-1".to_string()),
            ],
            resolved_thread_scoped_downstream_request_ids: vec![
                RequestId::String("downstream-thread-2".to_string()),
                RequestId::String("downstream-thread-1".to_string()),
            ],
            resolved_thread_scoped_methods: vec![
                "item/tool/requestUserInput".to_string(),
                "item/tool/requestUserInput".to_string(),
            ],
            resolved_thread_scoped_thread_ids: vec![
                "thread-visible".to_string(),
                "thread-visible".to_string(),
            ],
            stranded_connection_scoped_requests: 1,
            stranded_connection_scoped_request_ids: vec![RequestId::String(
                "gateway-connection-1".to_string(),
            )],
            stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
                "downstream-connection-1".to_string(),
            )],
            stranded_connection_scoped_methods: vec![
                "account/chatgptAuthTokens/refresh".to_string(),
            ],
        },
    );

    let event = operator_events_rx
        .try_recv()
        .expect("cleanup operator event should publish");
    assert_eq!(event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        serde_json::json!({
            "workerId": 1,
            "workerWebsocketUrl": "ws://worker-b.invalid",
            "remainingWorkerCount": 1,
            "disconnectMessage": "worker-b lost",
            "resolvedThreadScopedServerRequestCount": 2,
            "resolvedThreadScopedServerRequestIds": [
                "gateway-thread-1",
                "gateway-thread-2"
            ],
            "resolvedThreadScopedDownstreamServerRequestIds": [
                "downstream-thread-1",
                "downstream-thread-2"
            ],
            "resolvedThreadScopedServerRequestMethods": ["item/tool/requestUserInput"],
            "resolvedThreadScopedThreadIds": ["thread-visible"],
            "strandedConnectionScopedServerRequestCount": 1,
            "strandedConnectionScopedServerRequestIds": ["gateway-connection-1"],
            "strandedConnectionScopedDownstreamServerRequestIds": [
                "downstream-connection-1"
            ],
            "strandedConnectionScopedServerRequestMethods": [
                "account/chatgptAuthTokens/refresh"
            ],
        })
    );
}

#[test]
pub(crate) fn worker_server_request_cleanup_report_logs_and_publishes_operator_event() {
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let observability = GatewayObservability::default().with_operator_events(operator_events_tx);
    let cleanup = super::super::super::WorkerServerRequestCleanup {
        resolved_notifications: vec![super::super::super::WorkerCleanupResolvedNotification {
            notification: ServerRequestResolvedNotification {
                thread_id: "thread-visible".to_string(),
                request_id: RequestId::String("gateway-thread-1".to_string()),
            },
            method: "item/tool/requestUserInput".to_string(),
        }],
        resolved_thread_scoped_requests: 1,
        resolved_thread_scoped_request_ids: vec![RequestId::String("gateway-thread-1".to_string())],
        resolved_thread_scoped_downstream_request_ids: vec![RequestId::String(
            "downstream-thread-1".to_string(),
        )],
        resolved_thread_scoped_methods: vec!["item/tool/requestUserInput".to_string()],
        resolved_thread_scoped_thread_ids: vec!["thread-visible".to_string()],
        stranded_connection_scoped_requests: 1,
        stranded_connection_scoped_request_ids: vec![RequestId::String(
            "gateway-connection-1".to_string(),
        )],
        stranded_connection_scoped_downstream_request_ids: vec![RequestId::String(
            "downstream-connection-1".to_string(),
        )],
        stranded_connection_scoped_methods: vec!["account/chatgptAuthTokens/refresh".to_string()],
    };
    let report = super::super::super::WorkerServerRequestCleanupReport {
        worker_websocket_url: "ws://worker-b.invalid",
        remaining_worker_count: 1,
        disconnect_message: Some("worker-b lost"),
        message: "downstream worker disconnected within shared gateway v2 session",
    };

    let logs = capture_logs(|| {
        super::super::super::report_worker_server_request_cleanup(
            &observability,
            Some(1),
            &report,
            &cleanup,
        );
    });

    assert!(logs.contains("downstream worker disconnected within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    let event = operator_events_rx
        .try_recv()
        .expect("cleanup operator event should publish");
    assert_eq!(event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(
        event.data["resolvedThreadScopedServerRequestIds"],
        serde_json::json!(["gateway-thread-1"])
    );
    assert_eq!(
        event.data["strandedConnectionScopedServerRequestIds"],
        serde_json::json!(["gateway-connection-1"])
    );
}

#[test]
pub(crate) fn log_unexpected_client_server_request_response_includes_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_unexpected_client_server_request_response(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "response",
            &RequestId::String("gateway-unexpected".to_string()),
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("response"));
    assert!(logs.contains("unexpected_request_id=String(\"gateway-unexpected\")"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_unexpected_client_server_request_error_includes_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_unexpected_client_server_request_response(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "error",
            &RequestId::String("gateway-unexpected".to_string()),
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("error"));
    assert!(logs.contains("unexpected_request_id=String(\"gateway-unexpected\")"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_rejected_saturated_server_request_includes_scope_worker_and_pending_request_ids()
{
    let logs = capture_logs(|| {
        super::super::super::log_rejected_saturated_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("gateway-incoming".to_string()),
            "item/tool/requestUserInput",
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            2,
        );
    });

    assert!(logs.contains(
        "rejecting downstream server request because the gateway websocket connection is saturated"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("limit=2"));
    assert!(logs.contains("request_id=String(\"gateway-incoming\")"));
    assert!(logs.contains("item/tool/requestUserInput"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
}

#[test]
pub(crate) fn log_rejected_saturated_client_request_includes_scope_method_and_limit() {
    let logs = capture_logs(|| {
        super::super::super::log_rejected_saturated_client_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("command-exec-2".to_string()),
            "command/exec",
            1,
            1,
        );
    });

    assert!(logs.contains(
        "rejecting client request because the gateway websocket connection is saturated"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"command-exec-2\")"));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("limit=1"));
}

#[test]
pub(crate) fn log_notification_send_failure_includes_scope_worker_method_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::log_notification_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "warning",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver downstream notification to northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("method=\"warning\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_downstream_server_request_forward_failure_includes_scope_worker_and_method() {
    let logs = capture_logs(|| {
        super::super::super::log_downstream_server_request_forward_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "item/commandExecution/requestApproval",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver downstream server request to northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("method=\"item/commandExecution/requestApproval\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_client_response_send_failure_includes_scope_request_method_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::log_client_response_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("client-request-1".to_string()),
            "model/list",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(
        logs.contains("failed to deliver gateway v2 client request response to northbound client")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"client-request-1\")"));
    assert!(logs.contains("method=\"model/list\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_close_frame_send_failure_includes_scope_code_reason_and_outcome() {
    let logs = capture_logs(|| {
        super::super::super::log_close_frame_send_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            close_code::POLICY,
            "gateway initialize timed out",
            "client_send_timed_out",
            &std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "gateway websocket send timed out",
            ),
        );
    });

    assert!(logs.contains("failed to deliver gateway v2 close frame to northbound client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("code=1008"));
    assert!(logs.contains("reason=\"gateway initialize timed out\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert!(logs.contains("gateway websocket send timed out"));
}

#[test]
pub(crate) fn log_duplicate_pending_client_request_includes_scope_method_and_active_routes() {
    let logs = capture_logs(|| {
        super::super::super::log_duplicate_pending_client_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            &RequestId::String("command-exec-1".to_string()),
            "command/exec",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because the northbound client reused a pending request id"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("request_id=String(\"command-exec-1\")"));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_rejected_hidden_downstream_server_request_includes_scope_worker_and_thread_id() {
    let logs = capture_logs(|| {
        super::super::super::log_rejected_hidden_downstream_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-hidden".to_string(),
                project_id: Some("project-hidden".to_string()),
            },
            Some(3),
            "ws://worker-d.invalid",
            &RequestId::String("gateway-hidden".to_string()),
            "item/commandExecution/requestApproval",
            Some("thread-hidden"),
        );
    });

    assert!(logs.contains(
        "rejecting downstream server request for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-hidden"));
    assert!(logs.contains("project-hidden"));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
    assert!(logs.contains("request_id=String(\"gateway-hidden\")"));
    assert!(logs.contains("item/commandExecution/requestApproval"));
    assert!(logs.contains("thread-hidden"));
}

#[test]
pub(crate) fn log_downstream_backpressure_close_includes_scope_worker_and_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_downstream_backpressure_close(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            3,
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(2)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because the downstream app-server event stream lagged"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("skipped_event_count=3"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[2]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-c.invalid\"]")
    );
}

#[test]
pub(crate) fn log_client_send_timeout_includes_scope_detail_and_pending_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_client_send_timeout(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "gateway websocket send timed out",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "closing gateway v2 connection because sending to the northbound client timed out"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
    assert!(
        logs.contains("answered_but_unresolved_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
}

#[test]
pub(crate) fn log_downstream_shutdown_failure_includes_scope_outcome_and_pending_counts() {
    let logs = capture_logs(|| {
        super::super::super::log_downstream_shutdown_failure(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_send_timed_out",
            Some("gateway websocket send timed out"),
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
            &crate::v2_connection_health::GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 1,
                pending_client_request_worker_counts: vec![
                    crate::api::GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(1),
                        pending_client_request_count: 1,
                    },
                ],
                pending_client_request_method_counts: vec![
                    crate::api::GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 1,
                    },
                ],
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 3,
                server_request_backlog_worker_counts: vec![
                    crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 5,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 5,
                    },
                ],
            },
            &std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "downstream shutdown channel closed",
            ),
        );
    });

    assert!(
        logs.contains(
            "gateway v2 websocket downstream shutdown also failed after connection error"
        )
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_outcome=\"client_send_timed_out\""));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-1\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[1]"));
    assert!(
        logs.contains("pending_client_request_worker_websocket_urls=[\"ws://worker-b.invalid\"]")
    );
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_count: 1"));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=3"));
    assert!(logs.contains("server_request_backlog_count=5"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(2)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"item/tool/requestUserInput\""));
    assert!(logs.contains("downstream shutdown channel closed"));
}

#[test]
pub(crate) fn pending_client_responses_settle_completed_responses_before_teardown() {
    let (tx, mut rx) = mpsc::channel(2);
    let mut pending_client_responses = super::super::super::PendingClientResponses {
        tx,
        tasks: Vec::new(),
        count: 2,
        active: HashMap::from([
            (
                RequestId::String("command-exec-complete".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
            (
                RequestId::String("command-exec-pending".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(1),
                    worker_websocket_url: "ws://worker-b.invalid".to_string(),
                    started_at: Instant::now(),
                },
            ),
        ]),
    };

    pending_client_responses
        .tx
        .try_send(super::super::super::PendingClientResponse {
            request_id: RequestId::String("command-exec-complete".to_string()),
            request_context: GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            method: "command/exec".to_string(),
            started_at: Instant::now(),
            result: Ok(Ok(serde_json::json!({ "exitCode": 0 }))),
        })
        .expect("completed background response should enqueue");

    pending_client_responses.settle_completed_responses(&mut rx);

    assert_eq!(pending_client_responses.count, 1);
    assert_eq!(
        pending_client_responses
            .active
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![RequestId::String("command-exec-pending".to_string())]
    );
}

#[test]
pub(crate) fn log_aborted_pending_client_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_aborted_pending_client_requests(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "client_disconnected",
            Some("gateway websocket receive failed: closed"),
            &HashMap::from([
                (
                    RequestId::String("command-exec-1".to_string()),
                    super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(0),
                        worker_websocket_url: "ws://worker-a.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
                (
                    RequestId::String("command-exec-2".to_string()),
                    super::super::super::PendingClientRequestRoute {
                        method: "command/exec".to_string(),
                        request_context: GatewayRequestContext {
                            tenant_id: "tenant-visible".to_string(),
                            project_id: Some("project-visible".to_string()),
                        },
                        worker_id: Some(1),
                        worker_websocket_url: "ws://worker-b.invalid".to_string(),
                        started_at: Instant::now(),
                    },
                ),
            ]),
        );
    });

    assert!(logs.contains(
        "aborting pending gateway v2 client requests because the northbound connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("outcome=\"client_disconnected\""));
    assert!(logs.contains("detail=\"gateway websocket receive failed: closed\""));
    assert!(logs.contains("pending_client_request_count=2"));
    assert!(logs.contains(
        "pending_client_request_ids=[String(\"command-exec-1\"), String(\"command-exec-2\")]"
    ));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\", \"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[0, 1]"));
    assert!(logs.contains(
            "pending_client_request_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        ));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("worker_id: Some(0)"));
    assert!(logs.contains("worker_id: Some(1)"));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("pending_client_request_count: 2"));
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_request_metrics_and_audit_log() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_disconnected",
            &HashMap::from([(
                RequestId::String("command-exec-1".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_disconnected\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_disconnected", 1)]);
}

#[test]
pub(crate) fn observe_aborted_pending_client_requests_records_client_send_timeout_outcome() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);

    let logs = capture_logs(|| {
        super::super::super::observe_aborted_pending_client_requests(
            &observability,
            "client_send_timed_out",
            &HashMap::from([(
                RequestId::String("command-exec-timeout".to_string()),
                super::super::super::PendingClientRequestRoute {
                    method: "command/exec".to_string(),
                    request_context: GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    },
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid".to_string(),
                    started_at: Instant::now(),
                },
            )]),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"client_send_timed_out\""), "{logs}");
    assert_v2_request_metrics(&metrics, &[("command/exec", "client_send_timed_out", 1)]);
}

#[test]
pub(crate) fn log_rejected_pending_server_requests_includes_scope_outcome_and_request_ids() {
    let logs = capture_logs(|| {
        let worker_websocket_urls = [
            "ws://worker-a.invalid".to_string(),
            "ws://worker-b.invalid".to_string(),
        ];
        super::super::super::log_rejected_pending_server_requests(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "protocol_violation",
            Some("unexpected gateway websocket server-request response"),
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(1),
                    request_id: RequestId::String("downstream-resolved-1".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
            &worker_websocket_urls,
        );
    });

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("protocol_violation"));
    assert!(logs.contains("connection_detail"));
    assert!(logs.contains("unexpected gateway websocket server-request response"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=1"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(
        logs.contains("thread_scoped_pending_server_request_ids=[String(\"gateway-thread-1\")]")
    );
    assert!(logs.contains(
        "connection_scoped_pending_server_request_ids=[String(\"gateway-connection-1\")]"
    ));
    assert!(logs.contains("thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("worker_ids=[0, 1]"));
    assert!(
        logs.contains(
            "worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        )
    );
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains(
        "answered_but_unresolved_server_request_methods=[\"item/tool/requestUserInput\"]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
}

#[test]
pub(crate) fn log_duplicate_downstream_server_request_includes_scope_worker_and_pending_ids() {
    let logs = capture_logs(|| {
        super::super::super::log_duplicate_downstream_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("gateway-duplicate".to_string()),
            "item/tool/requestUserInput",
            &HashMap::from([
                (
                    RequestId::String("gateway-connection-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                        downstream_request_id: RequestId::String(
                            "downstream-connection-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: None,
                    },
                ),
                (
                    RequestId::String("gateway-thread-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(1),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        downstream_request_id: RequestId::String("downstream-thread-1".to_string()),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                ),
            ]),
        );
    });

    assert!(logs.contains(
            "closing gateway v2 connection because a downstream session reused a pending server-request id"
        ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("request_id=String(\"gateway-duplicate\")"));
    assert!(logs.contains("item/tool/requestUserInput"));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains(
            "pending_server_request_ids=[String(\"gateway-connection-1\"), String(\"gateway-thread-1\")]"
        ));
    assert!(logs.contains(
            "pending_downstream_server_request_ids=[String(\"downstream-connection-1\"), String(\"downstream-thread-1\")]"
        ));
    assert!(logs.contains("pending_server_request_methods=[\"item/tool/requestUserInput\"]"));
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0, 1]"));
    assert!(logs.contains(
        "pending_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
    ));
}

#[test]
pub(crate) fn log_dropped_duplicate_resolved_server_request_includes_scope_worker_and_routes() {
    let logs = capture_logs(|| {
        log_dropped_duplicate_resolved_server_request(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &RequestId::String("downstream-duplicate".to_string()),
            &HashMap::from([(
                super::super::super::DownstreamServerRequestKey {
                    worker_id: Some(2),
                    request_id: RequestId::String("downstream-remaining".to_string()),
                },
                super::super::super::ResolvedServerRequestRoute {
                    gateway_request_id: RequestId::String("gateway-remaining".to_string()),
                    worker_websocket_url: test_worker_websocket_url(Some(2)),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
            )]),
        );
    });

    assert!(logs.contains(
        "dropping duplicate downstream serverRequest/resolved replay after request-id translation"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("downstream_request_id=String(\"downstream-duplicate\")"));
    assert!(logs.contains("remaining_resolved_route_count=1"));
    assert!(
        logs.contains("remaining_resolved_gateway_request_ids=[String(\"gateway-remaining\")]")
    );
    assert!(
        logs.contains(
            "remaining_resolved_downstream_request_ids=[String(\"downstream-remaining\")]"
        )
    );
    assert!(
        logs.contains("remaining_resolved_server_request_methods=[\"item/tool/requestUserInput\"]")
    );
    assert!(logs.contains("remaining_resolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("remaining_resolved_worker_ids=[2]"));
    assert!(logs.contains("remaining_resolved_worker_websocket_urls=[\"ws://worker-c.invalid\"]"));
}

#[test]
pub(crate) fn log_suppressed_skills_changed_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::log_suppressed_skills_changed_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &JSONRPCNotification {
                method: "skills/changed".to_string(),
                params: Some(serde_json::json!({"source":"worker-b"})),
            },
        );
    });

    assert!(logs.contains(
            "suppressing duplicate multi-worker skills/changed notification until the client refreshes skills/list"
        ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("method=\"skills/changed\""));
    assert!(logs.contains("source"));
    assert!(logs.contains("worker-b"));
}

#[test]
pub(crate) fn log_suppressed_opted_out_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::log_suppressed_opted_out_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(1),
            "ws://worker-b.invalid",
            &JSONRPCNotification {
                method: "warning".to_string(),
                params: Some(serde_json::json!({
                    "threadId": null,
                    "message": "hidden by initialize capability",
                })),
            },
        );
    });

    assert!(logs.contains("suppressing downstream notification opted out by northbound v2 client"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("method=\"warning\""));
    assert!(logs.contains("hidden by initialize capability"));
}

#[test]
pub(crate) fn log_downstream_connect_protocol_violation_includes_scope_reason_and_message() {
    let logs = capture_logs(|| {
        super::super::super::log_downstream_connect_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "invalid_binary",
            "remote app server at `ws://worker-a.invalid` sent non-text initialize frame",
        );
    });

    assert!(
        logs.contains("downstream app-server sent a malformed v2 protocol frame during initialize")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("reason=\"invalid_binary\""));
    assert!(logs.contains("sent non-text initialize frame"));
}

#[test]
pub(crate) fn log_downstream_reconnect_protocol_violation_includes_scope_worker_reason_and_message()
{
    let logs = capture_logs(|| {
        super::super::super::log_downstream_reconnect_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            1,
            "ws://worker-b.invalid",
            "invalid_jsonrpc",
            "remote app server at `ws://worker-b.invalid` sent invalid initialize response",
        );
    });

    assert!(logs.contains(
        "downstream app-server sent a malformed v2 protocol frame during worker reconnect"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("reason=\"invalid_jsonrpc\""));
    assert!(logs.contains("sent invalid initialize response"));
}

#[test]
pub(crate) fn log_downstream_protocol_violation_includes_scope_worker_reason_and_routes() {
    let logs = capture_logs(|| {
        super::super::super::log_downstream_protocol_violation(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            "invalid_jsonrpc",
            "remote app server at `ws://worker-c.invalid` sent invalid JSON-RPC: expected value",
            2,
            &GatewayV2EventState {
                pending_server_requests: HashMap::from([(
                    RequestId::String("gateway-pending-1".to_string()),
                    PendingServerRequestRoute {
                        worker_id: Some(2),
                        worker_websocket_url: test_worker_websocket_url(Some(2)),
                        downstream_request_id: RequestId::String(
                            "downstream-pending-1".to_string(),
                        ),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                )]),
                resolved_server_requests: HashMap::from([(
                    super::super::super::DownstreamServerRequestKey {
                        worker_id: Some(1),
                        request_id: RequestId::String("downstream-resolved-1".to_string()),
                    },
                    super::super::super::ResolvedServerRequestRoute {
                        gateway_request_id: RequestId::String("gateway-resolved-1".to_string()),
                        worker_websocket_url: test_worker_websocket_url(Some(1)),
                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                    },
                )]),
                skills_changed_pending_refresh: false,
                forwarded_connection_notifications: HashMap::new(),
            },
        );
    });

    assert!(logs.contains("downstream app-server sent a malformed v2 protocol frame"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("reason=\"invalid_jsonrpc\""));
    assert!(logs.contains("sent invalid JSON-RPC"));
    assert!(logs.contains("active_worker_count=2"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"gateway-pending-1\")]"));
    assert!(
        logs.contains("pending_downstream_server_request_ids=[String(\"downstream-pending-1\")]")
    );
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[2]"));
    assert!(logs.contains("pending_worker_websocket_urls=[\"ws://worker-c.invalid\"]"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains("answered_but_unresolved_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[1]"));
}

#[test]
pub(crate) fn log_suppressed_duplicate_connection_notification_includes_scope_worker_and_params() {
    let logs = capture_logs(|| {
        super::super::super::log_suppressed_duplicate_connection_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            Some(0),
            "ws://worker-a.invalid",
            &JSONRPCNotification {
                method: "account/updated".to_string(),
                params: Some(serde_json::json!({"requiresOpenaiAuth":true})),
            },
        );
    });

    assert!(logs.contains("suppressing exact-duplicate multi-worker connection notification"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("original_worker_id=Some(0)"));
    assert!(logs.contains("original_worker_websocket_url=\"ws://worker-a.invalid\""));
    assert!(logs.contains("method=\"account/updated\""));
    assert!(logs.contains("requiresOpenaiAuth"));
    assert!(logs.contains("true"));
}

#[test]
pub(crate) fn log_suppressed_hidden_thread_notification_includes_scope_worker_thread_and_params() {
    let logs = capture_logs(|| {
        super::super::super::log_suppressed_hidden_thread_notification(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(3),
            "ws://worker-d.invalid",
            &JSONRPCNotification {
                method: "item/agentMessage/delta".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-hidden",
                    "turnId": "turn-hidden",
                    "itemId": "item-hidden",
                    "delta": "hidden text",
                })),
            },
        );
    });

    assert!(logs.contains(
        "suppressing downstream notification for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
    assert!(logs.contains("method=\"item/agentMessage/delta\""));
    assert!(logs.contains("thread-hidden"));
    assert!(logs.contains("hidden text"));
}

#[test]
pub(crate) fn log_deduplicated_thread_list_entry_includes_selected_and_discarded_workers() {
    let logs = capture_logs(|| {
        log_deduplicated_thread_list_entry(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            DeduplicatedThreadListEntryLog {
                thread_id: "thread-visible",
                selected_worker_id: Some(2),
                selected_worker_websocket_url: "ws://worker-c.invalid",
                discarded_worker_id: Some(7),
                discarded_worker_websocket_url: "ws://worker-h.invalid",
                selected_updated_at: 40,
                discarded_updated_at: 32,
                selected_created_at: 12,
                discarded_created_at: 8,
            },
        );
    });

    assert!(logs.contains("deduplicating repeated thread/list entry across downstream workers"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("selected_worker_id=Some(2)"));
    assert!(logs.contains("selected_worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("discarded_worker_id=Some(7)"));
    assert!(logs.contains("discarded_worker_websocket_url=\"ws://worker-h.invalid\""));
    assert!(logs.contains("selected_updated_at=40"));
    assert!(logs.contains("discarded_updated_at=32"));
    assert!(logs.contains("selected_created_at=12"));
    assert!(logs.contains("discarded_created_at=8"));
}

#[test]
pub(crate) fn log_recovered_visible_thread_worker_route_includes_scope_and_worker() {
    let logs = capture_logs(|| {
        super::super::super::log_recovered_visible_thread_worker_route(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            Some(3),
            "ws://worker-d.invalid",
        );
    });

    assert!(
        logs.contains("recovered missing visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("worker_id=Some(3)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-d.invalid\""));
}

#[test]
pub(crate) fn log_failed_visible_thread_worker_route_recovery_includes_attempted_workers() {
    let logs = capture_logs(|| {
        super::super::super::log_failed_visible_thread_worker_route_recovery(
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            "thread-visible",
            &[Some(1), Some(4), None],
            &[
                "ws://worker-b.invalid",
                "ws://worker-e.invalid",
                "<unknown>",
            ],
        );
    });

    assert!(
        logs.contains("failed to recover visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("attempted_worker_ids=[Some(1), Some(4), None]"));
    assert!(logs.contains(
            "attempted_worker_websocket_urls=[\"ws://worker-b.invalid\", \"ws://worker-e.invalid\", \"<unknown>\"]"
        ));
}

pub(crate) async fn start_mock_remote_server_expecting_forwarded_initialized() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let frame = websocket
            .next()
            .await
            .expect("forwarded initialized frame should exist")
            .expect("forwarded initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected forwarded initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("forwarded initialized should decode")
        else {
            panic!("expected forwarded initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_initialize_with_expected_headers(
    expected_tenant_id: &str,
    expected_project_id: Option<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_tenant_id = expected_tenant_id.to_string();
    let expected_project_id = expected_project_id.map(str::to_string);
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = accept_hdr_async(
            stream,
            move |request: &WebSocketRequest, response: WebSocketResponse| {
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-tenant-id")
                        .and_then(|value| value.to_str().ok()),
                    Some(expected_tenant_id.as_str())
                );
                assert_eq!(
                    request
                        .headers()
                        .get("x-codex-project-id")
                        .and_then(|value| value.to_str().ok()),
                    expected_project_id.as_deref()
                );
                Ok(response)
            },
        )
        .await
        .expect("websocket should accept");
        expect_remote_initialize(&mut websocket).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_hidden_server_request(
    request: JSONRPCRequest,
    expected_error_message: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let expected_request_id = request.id.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(request.clone()))
                    .expect("server request should serialize")
                    .into(),
            ))
            .await
            .expect("server request should send");

        let frame = websocket
            .next()
            .await
            .expect("server request response should exist")
            .expect("server request response should decode");
        let Message::Text(text) = frame else {
            panic!("expected server request response text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&text).expect("server request response should decode")
        else {
            panic!("expected server request error");
        };
        assert_eq!(error.id, expected_request_id);
        assert_eq!(error.error.code, super::super::super::INVALID_PARAMS_CODE);
        assert_eq!(error.error.message, expected_error_message);

        tokio::time::sleep(Duration::from_millis(250)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_thread_start_then_server_requests()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let frame = websocket
                        .next()
                        .await
                        .expect("initialized frame should exist")
                        .expect("initialized frame should decode");
                    let Message::Text(text) = frame else {
                        panic!("expected initialized text frame");
                    };
                    let JSONRPCMessage::Notification(notification) =
                        serde_json::from_str(&text).expect("initialized should decode")
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");

                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "thread/start");
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "thread": {
                                        "id": "thread-recovered",
                                        "forkedFromId": null,
                                        "preview": "",
                                        "ephemeral": true,
                                        "modelProvider": "openai",
                                        "createdAt": 1,
                                        "updatedAt": 1,
                                        "status": {
                                            "type": "idle",
                                        },
                                        "path": null,
                                        "cwd": "/tmp/recovered-worker",
                                        "cliVersion": "0.0.0-test",
                                        "source": "cli",
                                        "agentNickname": null,
                                        "agentRole": null,
                                        "gitInfo": null,
                                        "name": null,
                                        "turns": [],
                                    },
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": "/tmp/recovered-worker",
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess",
                                    },
                                    "reasoningEffort": null,
                                }),
                            }))
                            .expect("thread/start response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("thread/start response should send");

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-user-input".to_string()),
                                method: "item/tool/requestUserInput".to_string(),
                                params: Some(serde_json::json!({
                                    "threadId": "thread-recovered",
                                    "turnId": "turn-recovered",
                                    "itemId": "tool-call-recovered",
                                    "questions": [{
                                        "id": "mode",
                                        "header": "Mode",
                                        "question": "Pick execution mode",
                                        "isOther": false,
                                        "isSecret": false,
                                        "options": [],
                                    }],
                                })),
                                trace: None,
                            }))
                            .expect("user-input request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("user-input request should send");

                    let JSONRPCMessage::Response(user_input_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected user-input response");
                    };
                    assert_eq!(
                        user_input_response.id,
                        RequestId::String("downstream-user-input".to_string())
                    );
                    assert_eq!(
                        user_input_response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": ["safe"],
                                },
                            },
                        })
                    );

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                                id: RequestId::String("downstream-refresh".to_string()),
                                method: "account/chatgptAuthTokens/refresh".to_string(),
                                params: Some(serde_json::json!({
                                    "reason": "unauthorized",
                                    "previousAccountId": "acct-recovered",
                                })),
                                trace: None,
                            }))
                            .expect("refresh request should serialize")
                            .into(),
                        ))
                        .await
                        .expect("refresh request should send");

                    let JSONRPCMessage::Response(refresh_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected refresh response");
                    };
                    assert_eq!(
                        refresh_response.id,
                        RequestId::String("downstream-refresh".to_string())
                    );
                    assert_eq!(
                        refresh_response.result,
                        serde_json::json!({
                            "accessToken": "access-token-recovered",
                            "chatgptAccountId": "acct-recovered",
                            "chatgptPlanType": "pro",
                        })
                    );

                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_primary_login_completed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "account/login/start");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "type": "chatgpt",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "type": "chatgpt",
                                    "loginId": "login-reconnected",
                                    "authUrl": "https://example.com/login",
                                }),
                            }))
                            .expect("login response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("login response should send");
                    send_remote_notification(
                        &mut websocket,
                        "account/login/completed",
                        serde_json::json!({
                            "loginId": "login-reconnected",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_mcp_oauth_login_completed() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let request = read_websocket_request(&mut websocket).await;
                    assert_eq!(request.method, "mcpServer/oauth/login");
                    assert_json_params_eq(
                        request.params,
                        Some(serde_json::json!({
                            "name": "shared-mcp",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "authorizationUrl": "https://example.com/oauth/shared-mcp",
                                }),
                            }))
                            .expect("oauth response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("oauth response should send");
                    send_remote_notification(
                        &mut websocket,
                        "mcpServer/oauthLogin/completed",
                        serde_json::json!({
                            "name": "shared-mcp",
                            "success": true,
                            "error": null,
                        }),
                    )
                    .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    let JSONRPCMessage::Notification(initialized) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(initialized.method, "initialized");
                    assert_eq!(initialized.params, None);

                    let replay_watch_request = read_websocket_request(&mut websocket).await;
                    assert_eq!(replay_watch_request.method, "fs/watch");
                    assert_eq!(
                        replay_watch_request.id,
                        RequestId::String("gateway-replay-fs-watch:watch-shared".to_string())
                    );
                    assert_json_params_eq(
                        replay_watch_request.params,
                        Some(serde_json::json!({
                            "watchId": "watch-shared",
                            "path": "/tmp/shared/project/.git/HEAD",
                        })),
                    );
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: replay_watch_request.id,
                                result: serde_json::json!({
                                    "path": "/tmp/shared/project/.git/HEAD",
                                }),
                            }))
                            .expect("replayed fs/watch response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("replayed fs/watch response should send");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_initialized_replay_failure() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let JSONRPCMessage::Notification(initialized) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(initialized.method, "initialized");
        assert_eq!(initialized.params, None);

        websocket
            .close(None)
            .await
            .expect("close frame should send before replayed fs/watch");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_item_delta_notification() -> String {
    start_mock_remote_server_for_notification(ServerNotification::AgentMessageDelta(
        codex_app_server_protocol::AgentMessageDeltaNotification {
            thread_id: "thread-visible".to_string(),
            turn_id: "turn-visible".to_string(),
            item_id: "item-visible".to_string(),
            delta: "streamed text".to_string(),
        },
    ))
    .await
}

pub(crate) async fn start_mock_remote_server_for_notification(
    notification: ServerNotification,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let notification =
            tagged_type_to_notification(notification).expect("notification should serialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Notification(notification))
                    .expect("notification should serialize")
                    .into(),
            ))
            .await
            .expect("notification should send");

        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_realtime_start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let frame = websocket
            .next()
            .await
            .expect("realtime request should exist")
            .expect("realtime request should decode");
        let Message::Text(text) = frame else {
            panic!("expected realtime request text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("realtime request should decode")
        else {
            panic!("expected realtime request");
        };
        assert_eq!(request.method, "thread/realtime/start");
        assert_json_params_eq(
            request.params,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "outputModality": "text",
                "realtimeSessionId": null,
                "transport": {
                    "type": "websocket"
                },
                "voice": null,
            })),
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("realtime response should serialize")
                .into(),
            ))
            .await
            .expect("realtime response should send");

        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_realtime_append_text() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/appendText",
        serde_json::json!({
            "threadId": "thread-visible",
            "text": "hello realtime",
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_append_audio() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/appendAudio",
        serde_json::json!({
            "threadId": "thread-visible",
            "audio": {
                "data": "AQID",
                "sampleRate": 24000,
                "numChannels": 1,
                "samplesPerChannel": 3,
                "itemId": "item-visible",
            }
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_stop() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/stop",
        serde_json::json!({
            "threadId": "thread-visible",
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_started_notification() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        send_remote_notification(
            &mut websocket,
            "thread/realtime/started",
            serde_json::json!({
                "threadId": "thread-visible",
                "realtimeSessionId": "realtime-session-1",
                "version": "v1",
            }),
        )
        .await;

        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_realtime_notification(
    method: &str,
    params: serde_json::Value,
) -> String {
    start_mock_remote_server_for_realtime_notifications(vec![(method, params)]).await
}

pub(crate) async fn start_mock_remote_server_for_realtime_notifications(
    notifications: Vec<(&str, serde_json::Value)>,
) -> String {
    start_mock_remote_server_for_realtime_notifications_after_delay(notifications, Duration::ZERO)
        .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_notifications_after_delay(
    notifications: Vec<(&str, serde_json::Value)>,
    delay: Duration,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let notifications = notifications
        .into_iter()
        .map(|(method, params)| (method.to_string(), params))
        .collect::<Vec<_>>();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        tokio::time::sleep(delay).await;
        for (method, params) in notifications {
            send_remote_notification(&mut websocket, &method, params).await;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request(
    expected_method: &'static str,
    expected_params: serde_json::Value,
) -> String {
    start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
        expected_method,
        Some(expected_params),
        serde_json::json!({}),
    )
    .await
}

pub(crate) fn canonicalize_json_params(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("version")
                && map.contains_key("threadId")
                && let Some(session_id) = map.remove("sessionId")
            {
                map.entry("realtimeSessionId".to_string())
                    .or_insert(session_id);
            }
            map.retain(|key, value| {
                if value.is_null() {
                    return false;
                }
                if matches!(
                    key.as_str(),
                    "refreshToken"
                        | "experimentalRawEvents"
                        | "persistExtendedHistory"
                        | "includeLogs"
                        | "useStateDbOnly"
                ) && value == &serde_json::Value::Bool(false)
                {
                    return false;
                }
                if key == "includeTurns" && value == &serde_json::Value::Bool(false) {
                    return false;
                }
                if key == "role" && value == &serde_json::Value::String("user".to_string()) {
                    return false;
                }
                true
            });
            for value in map.values_mut() {
                canonicalize_json_params(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize_json_params(value);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

pub(crate) fn canonicalize_bootstrap_response_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("installPolicy")
                && map.contains_key("authPolicy")
                && map.contains_key("interface")
            {
                if map.get("availability")
                    == Some(&serde_json::Value::String("AVAILABLE".to_string()))
                {
                    map.remove("availability");
                }
                if map.get("keywords") == Some(&serde_json::Value::Array(Vec::new())) {
                    map.remove("keywords");
                }
                if map
                    .get("localVersion")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("localVersion");
                }
                if map
                    .get("remotePluginId")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("remotePluginId");
                }
                if map
                    .get("shareContext")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("shareContext");
                }
            }
            if map.contains_key("authStatus")
                && map.contains_key("resourceTemplates")
                && map.contains_key("resources")
                && map.contains_key("tools")
                && map
                    .get("serverInfo")
                    .is_some_and(serde_json::Value::is_null)
            {
                map.remove("serverInfo");
            }
            for value in map.values_mut() {
                canonicalize_bootstrap_response_json(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize_bootstrap_response_json(value);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

pub(crate) fn assert_json_params_eq(
    actual: Option<serde_json::Value>,
    expected: Option<serde_json::Value>,
) {
    let mut actual = actual;
    let mut expected = expected;
    if let Some(actual) = &mut actual {
        canonicalize_json_params(actual);
    }
    if let Some(expected) = &mut expected {
        canonicalize_json_params(expected);
    }
    assert_eq!(actual, expected);
}

pub(crate) async fn start_mock_remote_server_for_single_request(
    request_tx: oneshot::Sender<JSONRPCRequest>,
    response_result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        request_tx
            .send(request.clone())
            .expect("request should be captured");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: response_result,
                }))
                .expect("response should serialize")
                .into(),
            ))
            .await
            .expect("response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_result(
    expected_method: &'static str,
    expected_params: serde_json::Value,
    response_result: serde_json::Value,
) -> String {
    start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
        expected_method,
        Some(expected_params),
        response_result,
    )
    .await
}

pub(crate) fn plugin_summary_json(
    id: &str,
    installed: bool,
    enabled: bool,
    short_description: &str,
) -> PluginSummary {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": id.strip_suffix("@local").unwrap_or(id),
        "source": {
            "type": "local",
            "path": format!("/tmp/project/plugins/{id}"),
        },
        "installed": installed,
        "enabled": enabled,
        "availability": "AVAILABLE",
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_USE",
        "keywords": [],
        "localVersion": null,
        "remotePluginId": null,
        "shareContext": null,
        "interface": {
            "displayName": id,
            "shortDescription": short_description,
            "longDescription": null,
            "developerName": null,
            "category": null,
            "capabilities": [],
            "websiteUrl": null,
            "privacyPolicyUrl": null,
            "termsOfServiceUrl": null,
            "defaultPrompt": null,
            "brandColor": null,
            "composerIcon": null,
            "composerIconUrl": null,
            "logo": null,
            "logoUrl": null,
            "screenshots": [],
            "screenshotUrls": [],
        },
    }))
    .expect("plugin summary should deserialize")
}

pub(crate) async fn start_mock_remote_server_for_paginated_passthrough_requests(
    expected_method: &'static str,
    expected_requests_and_results: Vec<(serde_json::Value, serde_json::Value)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        for (expected_params, response_result) in expected_requests_and_results {
            let request = read_websocket_request(&mut websocket).await;
            assert_eq!(request.method, expected_method);
            assert_json_params_eq(request.params, Some(expected_params));

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: response_result,
                    }))
                    .expect("response should serialize")
                    .into(),
                ))
                .await
                .expect("response should send");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_error(
    expected_method: &'static str,
    expected_params: serde_json::Value,
    response_error: JSONRPCErrorError,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, expected_method);
        assert_json_params_eq(request.params, Some(expected_params));

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: request.id,
                    error: response_error,
                }))
                .expect("error response should serialize")
                .into(),
            ))
            .await
            .expect("error response should send");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
    expected_method: &'static str,
    expected_params: Option<serde_json::Value>,
    response_result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, expected_method);
        assert_json_params_eq(request.params, expected_params);

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: response_result,
                }))
                .expect("response should serialize")
                .into(),
            ))
            .await
            .expect("response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) fn pending_command_exec_params(process_id: &str) -> serde_json::Value {
    serde_json::json!({
        "command": ["sh", "-lc", "sleep 1"],
        "processId": process_id,
        "tty": true,
        "streamStdin": true,
        "streamStdoutStderr": true,
        "outputBytesCap": null,
        "timeoutMs": null,
        "cwd": null,
        "env": null,
        "size": {
            "rows": 24,
            "cols": 80,
        },
        "sandboxPolicy": null,
    })
}

pub(crate) async fn start_mock_remote_server_for_pending_command_exec(
    first_request_observed_tx: oneshot::Sender<()>,
    finish_first_request_rx: oneshot::Receiver<()>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, "command/exec");
        assert_json_params_eq(
            request.params,
            Some(pending_command_exec_params("proc-pending-1")),
        );
        first_request_observed_tx
            .send(())
            .expect("first command/exec observation should send");
        finish_first_request_rx
            .await
            .expect("first command/exec completion signal should arrive");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({
                        "exitCode": 0,
                        "stdout": "",
                        "stderr": "",
                    }),
                }))
                .expect("command/exec response should serialize")
                .into(),
            ))
            .await
            .expect("command/exec response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_review_start_then_thread_read() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let review_start = websocket
            .next()
            .await
            .expect("review/start request should exist")
            .expect("review/start request should decode");
        let Message::Text(review_start_text) = review_start else {
            panic!("expected review/start text frame");
        };
        let JSONRPCMessage::Request(review_start_request) =
            serde_json::from_str(&review_start_text).expect("review/start should decode")
        else {
            panic!("expected review/start request");
        };
        assert_eq!(review_start_request.method, "review/start");
        assert_json_params_eq(
            review_start_request.params,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "target": {
                    "type": "custom",
                    "instructions": "Review the current change",
                },
                "delivery": "detached",
            })),
        );
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: review_start_request.id,
                    result: serde_json::json!({
                        "turn": {
                            "id": "turn-review",
                            "items": [],
                            "status": "pending",
                            "error": null,
                            "startedAt": 1,
                            "completedAt": null,
                            "durationMs": null,
                        },
                        "reviewThreadId": "thread-review",
                    }),
                }))
                .expect("review/start response should serialize")
                .into(),
            ))
            .await
            .expect("review/start response should send");

        let thread_read = websocket
            .next()
            .await
            .expect("thread/read request should exist")
            .expect("thread/read request should decode");
        let Message::Text(thread_read_text) = thread_read else {
            panic!("expected thread/read text frame");
        };
        let JSONRPCMessage::Request(thread_read_request) =
            serde_json::from_str(&thread_read_text).expect("thread/read should decode")
        else {
            panic!("expected thread/read request");
        };
        assert_eq!(thread_read_request.method, "thread/read");
        assert_json_params_eq(
            thread_read_request.params,
            Some(serde_json::json!({
                "threadId": "thread-review",
                "includeTurns": false,
            })),
        );
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: thread_read_request.id,
                    result: serde_json::json!({
                        "thread": {
                            "id": "thread-review",
                            "name": "Detached review thread",
                        },
                    }),
                }))
                .expect("thread/read response should serialize")
                .into(),
            ))
            .await
            .expect("thread/read response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_thread_fork_and_read() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };

                    let result = match request.method.as_str() {
                        "thread/fork" => serde_json::json!({
                            "thread": {
                                "id": "thread-forked",
                                "name": "Forked thread",
                                "cwd": "/tmp/worker-b",
                            },
                            "model": "gpt-5",
                            "modelProvider": "openai",
                            "serviceTier": null,
                            "cwd": "/tmp/worker-b",
                            "instructionSources": [],
                            "approvalPolicy": "on-request",
                            "approvalsReviewer": "user",
                            "sandbox": { "type": "dangerFullAccess" },
                            "reasoningEffort": null,
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": "thread-forked",
                                "name": "Forked thread",
                                "cwd": "/tmp/worker-b",
                            },
                        }),
                        other => panic!("unexpected reconnectable thread method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_review_start_then_thread_read()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };

                    let result = match request.method.as_str() {
                        "review/start" => serde_json::json!({
                            "turn": {
                                "id": "turn-review",
                                "items": [],
                                "status": "pending",
                                "error": null,
                                "startedAt": 1,
                                "completedAt": null,
                                "durationMs": null,
                            },
                            "reviewThreadId": "thread-review",
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": "thread-review",
                                "name": "Detached review thread",
                            },
                        }),
                        other => panic!("unexpected reconnectable review method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_review_start_then_thread_read()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
                        let frame = frame.expect("frame should decode");
                        let Message::Text(text) = frame else {
                            continue;
                        };
                        let JSONRPCMessage::Request(request) =
                            serde_json::from_str(&text).expect("request should decode")
                        else {
                            continue;
                        };

                        let result = match request.method.as_str() {
                            "review/start" => serde_json::json!({
                                "turn": {
                                    "id": "turn-review",
                                    "items": [],
                                    "status": "pending",
                                    "error": null,
                                    "startedAt": 1,
                                    "completedAt": null,
                                    "durationMs": null,
                                },
                                "reviewThreadId": "thread-review",
                            }),
                            "thread/read" => serde_json::json!({
                                "thread": {
                                    "id": "thread-review",
                                    "name": "Detached review thread",
                                },
                            }),
                            other => {
                                panic!("unexpected reconnectable review method: {other}")
                            }
                        };

                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result,
                                }))
                                .expect("response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_thread_list_and_read(
    thread_id: &str,
    thread_name: &str,
    cwd: &str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let thread_id = thread_id.to_string();
    let thread_name = thread_name.to_string();
    let cwd = cwd.to_string();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let thread_id = thread_id.clone();
            let thread_name = thread_name.clone();
            let cwd = cwd.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };

                    let result = match request.method.as_str() {
                        "thread/list" => serde_json::json!({
                            "data": [{
                                "id": thread_id,
                                "sessionId": thread_id,
                                "forkedFromId": null,
                                "preview": "",
                                "ephemeral": true,
                                "modelProvider": "openai",
                                "createdAt": if thread_id == "thread-worker-a" { 1 } else { 2 },
                                "updatedAt": if thread_id == "thread-worker-a" { 1 } else { 2 },
                                "status": { "type": "idle" },
                                "path": null,
                                "cwd": cwd,
                                "cliVersion": "0.0.0-test",
                                "source": "cli",
                                "agentNickname": null,
                                "agentRole": null,
                                "gitInfo": null,
                                "name": thread_name,
                                "turns": [],
                            }],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        "thread/read" => serde_json::json!({
                            "thread": {
                                "id": thread_id,
                                "name": thread_name,
                                "cwd": cwd,
                            },
                        }),
                        "thread/resume" => serde_json::json!({
                            "thread": {
                                "id": thread_id,
                                "name": thread_name,
                                "cwd": cwd,
                            },
                        }),
                        other => panic!("unexpected request method: {other}"),
                    };

                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) fn reconnectable_thread_list_entry_json(
    thread_id: &str,
    thread_name: &str,
    cwd: &str,
    timestamp: i64,
) -> serde_json::Value {
    serde_json::json!({
        "id": thread_id,
        "sessionId": thread_id,
        "forkedFromId": null,
        "preview": "",
        "ephemeral": true,
        "modelProvider": "openai",
        "createdAt": timestamp,
        "updatedAt": timestamp,
        "status": { "type": "idle" },
        "path": null,
        "cwd": cwd,
        "cliVersion": "0.0.0-test",
        "source": "cli",
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": thread_name,
        "turns": [],
    })
}

pub(crate) async fn start_mock_remote_server_for_connection_notification(
    method: &str,
    params: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let method = method.to_string();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        let params = if method == "externalAgentConfig/import/completed" {
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            })
        } else {
            params
        };
        send_remote_notification(&mut websocket, &method, params).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_idle_session() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_skills_changed_and_list(
    cwd: &str,
    skills: Vec<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let cwd = cwd.to_string();
    let skills = skills
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": format!("{name} description"),
                "path": format!("{cwd}/{name}"),
                "scope": "repo",
                "enabled": true,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        loop {
            let Some(frame) = websocket.next().await else {
                break;
            };
            let frame = frame.expect("frame should decode");
            let Message::Text(text) = frame else {
                continue;
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("request should decode")
            else {
                continue;
            };
            assert_eq!(request.method, "skills/list");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({
                            "data": [{
                                "cwd": cwd.clone(),
                                "skills": skills.clone(),
                                "errors": [],
                            }]
                        }),
                    }))
                    .expect("skills/list response should serialize")
                    .into(),
                ))
                .await
                .expect("skills/list response should send");
            send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_skills_changed_and_failing_list() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: request.id,
                    error: JSONRPCErrorError {
                        code: super::super::super::INTERNAL_ERROR_CODE,
                        message: "skills list failed".to_string(),
                        data: None,
                    },
                }))
                .expect("skills/list error should serialize")
                .into(),
            ))
            .await
            .expect("skills/list error should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_skills_changed_failing_then_successful_list()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let failed_request = read_websocket_request(&mut websocket).await;
        assert_eq!(failed_request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: failed_request.id,
                    error: JSONRPCErrorError {
                        code: super::super::super::INTERNAL_ERROR_CODE,
                        message: "first skills list failed".to_string(),
                        data: None,
                    },
                }))
                .expect("skills/list error should serialize")
                .into(),
            ))
            .await
            .expect("skills/list error should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let successful_request = read_websocket_request(&mut websocket).await;
        assert_eq!(successful_request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: successful_request.id,
                    result: serde_json::json!({
                        "data": [{
                            "cwd": "/tmp/worker-a",
                            "skills": [{
                                "name": "skill-a",
                                "description": "skill-a description",
                                "path": "/tmp/worker-a/skill-a",
                                "scope": "repo",
                                "enabled": true,
                            }],
                            "errors": [],
                        }]
                    }),
                }))
                .expect("skills/list response should serialize")
                .into(),
            ))
            .await
            .expect("skills/list response should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_skills_list(
    cwd: &str,
    skills: Vec<&str>,
) -> String {
    let cwd = cwd.to_string();
    let skills = skills
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": format!("{name} description"),
                "path": format!("{cwd}/{name}"),
                "scope": "repo",
                "enabled": true,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "skills/list",
        serde_json::json!({
            "data": [{
                "cwd": cwd,
                "skills": skills,
                "errors": [],
            }]
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_request(
    method: &'static str,
    result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };
                    if request.method == "configRequirements/read" {
                        assert_eq!(request.params, None);
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "requirements": null,
                                    }),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                        continue;
                    }
                    assert_eq!(request.method, method);
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: result.clone(),
                            }))
                            .expect("reconnectable response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable response should send");
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
    method: &'static str,
    params: serde_json::Value,
    result: serde_json::Value,
) -> String {
    start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
            method,
            Some(params),
            result,
        )
        .await
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
    method: &'static str,
    params: Option<serde_json::Value>,
    result: serde_json::Value,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
            let params = params.clone();
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
                        let frame = frame.expect("frame should decode");
                        let Message::Text(text) = frame else {
                            continue;
                        };
                        let JSONRPCMessage::Request(request) =
                            serde_json::from_str(&text).expect("request should decode")
                        else {
                            continue;
                        };
                        assert_eq!(request.method, method);
                        assert_json_params_eq(request.params, params.clone());
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: result.clone(),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_request_with_recording(
    method: &'static str,
    result: serde_json::Value,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };
                    if request.method == "configRequirements/read" {
                        assert_eq!(request.params, None);
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "requirements": null,
                                    }),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                        continue;
                    }
                    assert_eq!(request.method, method);
                    requests.lock().await.push(request.method.clone());
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: result.clone(),
                            }))
                            .expect("reconnectable response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable response should send");
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
    method: &'static str,
    result: serde_json::Value,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let result = result.clone();
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
                        let frame = frame.expect("frame should decode");
                        let Message::Text(text) = frame else {
                            continue;
                        };
                        let JSONRPCMessage::Request(request) =
                            serde_json::from_str(&text).expect("request should decode")
                        else {
                            continue;
                        };
                        assert_eq!(request.method, method);
                        requests.lock().await.push(request.method.clone());
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: result.clone(),
                                }))
                                .expect("reconnectable response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                loop {
                    let Some(frame) = websocket.next().await else {
                        break;
                    };
                    let frame = frame.expect("frame should decode");
                    let Message::Text(text) = frame else {
                        continue;
                    };
                    let JSONRPCMessage::Request(request) =
                        serde_json::from_str(&text).expect("request should decode")
                    else {
                        continue;
                    };
                    requests.lock().await.push(request.method.clone());
                    let result = match request.method.as_str() {
                        "fs/watch" => serde_json::json!({
                            "path": request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("path"))
                                .cloned()
                                .expect("fs/watch should include path"),
                        }),
                        "fs/unwatch" => serde_json::json!({}),
                        method => panic!("unexpected reconnectable fs method: {method}"),
                    };
                    websocket
                        .send(Message::Text(
                            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }))
                            .expect("reconnectable fs response should serialize")
                            .into(),
                        ))
                        .await
                        .expect("reconnectable fs response should send");
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_and_unwatch()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => loop {
                        let Some(frame) = websocket.next().await else {
                            break;
                        };
                        let frame = frame.expect("frame should decode");
                        let Message::Text(text) = frame else {
                            continue;
                        };
                        let JSONRPCMessage::Request(request) =
                            serde_json::from_str(&text).expect("request should decode")
                        else {
                            continue;
                        };
                        requests.lock().await.push(request.method.clone());
                        let result = match request.method.as_str() {
                            "fs/watch" => serde_json::json!({
                                "path": request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("path"))
                                    .cloned()
                                    .expect("fs/watch should include path"),
                            }),
                            "fs/unwatch" => serde_json::json!({}),
                            method => panic!("unexpected reconnectable fs method: {method}"),
                        };
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result,
                                }))
                                .expect("reconnectable fs response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("reconnectable fs response should send");
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_with_changed_notification()
-> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_task = Arc::clone(&requests);
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let requests = Arc::clone(&requests_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        let request = read_websocket_request(&mut websocket).await;
                        assert_eq!(request.method, "fs/watch");
                        requests.lock().await.push(request.method.clone());
                        let params = request
                            .params
                            .clone()
                            .expect("fs/watch should include params");
                        let path = params
                            .get("path")
                            .cloned()
                            .expect("fs/watch should include path");
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "path": path,
                                    }),
                                }))
                                .expect("fs/watch response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("fs/watch response should send");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        send_remote_notification(
                            &mut websocket,
                            "fs/changed",
                            serde_json::json!({
                                "watchId": "watch-reconnected",
                                "changedPaths": ["/tmp/shared/project/.git/HEAD"],
                            }),
                        )
                        .await;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    (format!("ws://{addr}"), requests)
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_app_list(
    apps: Vec<(&str, &str)>,
) -> String {
    let apps = apps
        .into_iter()
        .map(|(id, name)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "description": format!("{name} description"),
                "installUrl": null,
                "needsAuth": false,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "app/list",
        serde_json::json!({
            "data": apps,
            "nextCursor": null,
        }),
    )
    .await
}

pub(crate) fn reconnectable_model_json(
    id: &str,
    display_name: &str,
    is_default: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "model": id,
        "upgrade": null,
        "upgradeInfo": null,
        "availabilityNux": null,
        "displayName": display_name,
        "description": format!("{display_name} description"),
        "hidden": false,
        "defaultServiceTier": null,
        "serviceTiers": [],
        "supportedReasoningEfforts": [{
            "reasoningEffort": "medium",
            "description": "Balanced",
        }],
        "defaultReasoningEffort": "medium",
        "inputModalities": ["text"],
        "supportsPersonality": false,
        "additionalSpeedTiers": [],
        "isDefault": is_default,
    })
}

pub(crate) async fn start_mock_remote_server_for_paginated_model_list(
    pages: Vec<(Option<String>, serde_json::Value)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        while let Some(message) = websocket.next().await {
            let Message::Text(text) = message.expect("model/list request should decode") else {
                continue;
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("model/list request should deserialize")
            else {
                panic!("expected model/list request");
            };
            assert_eq!(request.method, "model/list");
            assert_eq!(
                request
                    .params
                    .as_ref()
                    .and_then(|params| params.get("limit")),
                Some(&Value::Null)
            );
            assert_eq!(
                request
                    .params
                    .as_ref()
                    .and_then(|params| params.get("includeHidden"))
                    .and_then(Value::as_bool),
                Some(true)
            );
            let cursor = request
                .params
                .as_ref()
                .and_then(|params| params.get("cursor"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let response = pages
                .iter()
                .find(|(page_cursor, _)| *page_cursor == cursor)
                .map(|(_, response)| response.clone())
                .unwrap_or_else(|| panic!("unexpected model/list cursor: {cursor:?}"));
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: response,
                    }))
                    .expect("model/list response should serialize")
                    .into(),
                ))
                .await
                .expect("model/list response should send");
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_mcp_server_status_list(
    names: Vec<&str>,
) -> String {
    let statuses = names
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken",
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "mcpServerStatus/list",
        serde_json::json!({
            "data": statuses,
            "nextCursor": null,
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_disconnect_then_reconnectable_mcp_status_with_startup_notification()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..2 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket should accept");

                expect_remote_initialize(&mut websocket).await;

                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        let request = read_websocket_request(&mut websocket).await;
                        assert_eq!(request.method, "mcpServerStatus/list");
                        websocket
                            .send(Message::Text(
                                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "name": "worker-b-mcp",
                                            "tools": {},
                                            "resources": [],
                                            "resourceTemplates": [],
                                            "authStatus": "bearerToken",
                                        }],
                                        "nextCursor": null,
                                    }),
                                }))
                                .expect("mcp status response should serialize")
                                .into(),
                            ))
                            .await
                            .expect("mcp status response should send");
                        send_remote_notification(
                            &mut websocket,
                            "mcpServer/startupStatus/updated",
                            serde_json::json!({
                                "name": "worker-b-mcp",
                                "status": "ready",
                                "error": null,
                            }),
                        )
                        .await;
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            });
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn spawn_remote_gateway_v2_test_server(
    websocket_url: String,
    scope_registry: Arc<GatewayScopeRegistry>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let initialize_response = test_initialize_response().await;
    spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await
}

pub(crate) async fn start_mock_remote_server_that_disconnects_after_initialize() -> String {
    start_mock_remote_server_that_disconnects_after_initialize_with_reason(String::new()).await
}

pub(crate) async fn start_mock_remote_server_that_stays_connected_after_initialize() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_invalid_jsonrpc_after_initialize() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid JSON-RPC should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_binary_during_initialize() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Binary(vec![0, 1, 2].into()))
            .await
            .expect("binary frame should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_server_request_during_initialize()
-> (String, oneshot::Receiver<JSONRPCResponse>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (response_tx, response_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");

        let server_request_id = RequestId::String("srv-init".to_string());
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: server_request_id.clone(),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-init",
                        "turnId": "turn-init",
                        "itemId": "item-init",
                        "questions": [{
                            "id": "question-init",
                            "header": "Mode",
                            "question": "Pick one",
                            "isOther": false,
                            "isSecret": false,
                            "options": [],
                        }],
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected server request response");
        };
        assert_eq!(response.id, server_request_id);
        response_tx
            .send(response)
            .expect("test should wait for server request response");
    });
    (format!("ws://{addr}"), response_rx)
}

pub(crate) async fn start_mock_remote_server_that_sends_unknown_server_request_during_initialize()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(initialize_request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(initialize_request.method, "initialize");

        let server_request_id = RequestId::String("srv-init-unknown".to_string());
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: server_request_id.clone(),
                    method: "thread/unknown".to_string(),
                    params: None,
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected setup-time server request rejection");
        };
        assert_eq!(error.id, server_request_id);
        assert_eq!(error.error.code, -32601);
        assert_eq!(
            error.error.message,
            "unsupported remote app-server request `thread/unknown`"
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: initialize_request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");

        let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
            panic!("expected follow-up request");
        };
        assert_eq!(request.method, "model/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({ "models": [] }),
                }))
                .expect("model list response should serialize")
                .into(),
            ))
            .await
            .expect("model list response should send");
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_invalid_jsonrpc_during_initialize() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid JSON-RPC should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_wrong_id_response_during_initialize()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: RequestId::String("not-initialize".to_string()),
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("wrong-id initialize response should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_wrong_id_error_during_initialize() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: RequestId::String("not-initialize".to_string()),
                    error: JSONRPCErrorError {
                        code: -32603,
                        message: "unexpected initialize error".to_string(),
                        data: None,
                    },
                }))
                .expect("initialize error should serialize")
                .into(),
            ))
            .await
            .expect("wrong-id initialize error should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_binary_after_initialize() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");
        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
        websocket
            .send(Message::Binary(vec![0, 1, 2].into()))
            .await
            .expect("binary frame should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_unexpected_response_after_initialize()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");
        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: RequestId::String("unexpected-response".to_string()),
                    result: serde_json::json!({}),
                }))
                .expect("unexpected response should serialize")
                .into(),
            ))
            .await
            .expect("unexpected response should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_sends_unexpected_error_after_initialize() -> String
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");
        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: RequestId::String("unexpected-error".to_string()),
                    error: JSONRPCErrorError {
                        code: -32603,
                        message: "unexpected".to_string(),
                        data: None,
                    },
                }))
                .expect("unexpected error should serialize")
                .into(),
            ))
            .await
            .expect("unexpected error should send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_that_disconnects_after_initialize_with_reason(
    reason: String,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .close((!reason.is_empty()).then_some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Error,
                    reason: reason.into(),
                },
            ))
            .await
            .expect("close frame should send");
    });
    format!("ws://{addr}")
}

pub(crate) async fn send_remote_notification(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    method: &str,
    params: serde_json::Value,
) {
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: method.to_string(),
                params: Some(params),
            }))
            .expect("notification should serialize")
            .into(),
        ))
        .await
        .expect("notification should send");
}

pub(crate) fn assert_jsonrpc_notification(
    message: JSONRPCMessage,
    expected_method: &str,
    expected_params: serde_json::Value,
) {
    let JSONRPCMessage::Notification(notification) = message else {
        panic!("expected notification");
    };
    assert_eq!(notification.method, expected_method);
    assert_json_params_eq(notification.params, Some(expected_params));
}

pub(crate) async fn expect_remote_initialize(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let frame = websocket
        .next()
        .await
        .expect("initialize frame should exist")
        .expect("initialize frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected initialize text frame");
    };
    let JSONRPCMessage::Request(request) =
        serde_json::from_str(&text).expect("initialize should decode")
    else {
        panic!("expected initialize request");
    };
    assert_eq!(request.method, "initialize");
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }))
            .expect("initialize response should serialize")
            .into(),
        ))
        .await
        .expect("initialize response should send");

    let frame = websocket
        .next()
        .await
        .expect("initialized frame should exist")
        .expect("initialized frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected initialized text frame");
    };
    let JSONRPCMessage::Notification(notification) =
        serde_json::from_str(&text).expect("initialized should decode")
    else {
        panic!("expected initialized notification");
    };
    assert_eq!(notification.method, "initialized");
}

pub(crate) async fn expect_remote_initialize_split<S, R>(write: &mut S, read: &mut R)
where
    S: futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    R: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let frame = read
        .next()
        .await
        .expect("initialize frame should exist")
        .expect("initialize frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected initialize text frame");
    };
    let JSONRPCMessage::Request(request) =
        serde_json::from_str(&text).expect("initialize should decode")
    else {
        panic!("expected initialize request");
    };
    assert_eq!(request.method, "initialize");
    write
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }))
            .expect("initialize response should serialize")
            .into(),
        ))
        .await
        .expect("initialize response should send");

    let frame = read
        .next()
        .await
        .expect("initialized frame should exist")
        .expect("initialized frame should decode");
    let Message::Text(text) = frame else {
        panic!("expected initialized text frame");
    };
    let JSONRPCMessage::Notification(notification) =
        serde_json::from_str(&text).expect("initialized should decode")
    else {
        panic!("expected initialized notification");
    };
    assert_eq!(notification.method, "initialized");
}
