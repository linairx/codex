use super::*;

#[test]
fn collect_server_request_cleanup_for_worker_tracks_thread_and_connection_scoped_routes() {
    let worker_id = Some(3);
    let other_worker_id = Some(9);
    let mut pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-thread-pending".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id,
                worker_websocket_url: test_worker_websocket_url(worker_id),
                downstream_request_id: RequestId::String("downstream-thread-pending".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            RequestId::String("gateway-connection-pending".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id,
                worker_websocket_url: test_worker_websocket_url(worker_id),
                downstream_request_id: RequestId::String(
                    "downstream-connection-pending".to_string(),
                ),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
        (
            RequestId::String("gateway-other-worker".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: other_worker_id,
                worker_websocket_url: test_worker_websocket_url(other_worker_id),
                downstream_request_id: RequestId::String("downstream-other-worker".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-other".to_string()),
            },
        ),
    ]);
    let mut resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id,
                request_id: RequestId::String("downstream-thread-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-thread-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id,
                request_id: RequestId::String("downstream-connection-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-connection-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: other_worker_id,
                request_id: RequestId::String("downstream-other-worker-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-other-worker-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(other_worker_id),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-other".to_string()),
            },
        ),
    ]);

    let cleanup = collect_server_request_cleanup_for_worker(
        &mut pending_server_requests,
        &mut resolved_server_requests,
        worker_id,
    );

    pretty_assertions::assert_eq!(cleanup.resolved_thread_scoped_requests, 2);
    pretty_assertions::assert_eq!(cleanup.stranded_connection_scoped_requests, 2);
    pretty_assertions::assert_eq!(
        cleanup.resolved_thread_scoped_request_ids,
        vec![
            RequestId::String("gateway-thread-pending".to_string()),
            RequestId::String("gateway-thread-resolved".to_string()),
        ]
    );
    pretty_assertions::assert_eq!(
        cleanup.resolved_thread_scoped_thread_ids,
        vec!["thread-owned".to_string(), "thread-owned".to_string()]
    );
    pretty_assertions::assert_eq!(
        cleanup.resolved_thread_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    pretty_assertions::assert_eq!(
        cleanup.stranded_connection_scoped_request_ids,
        vec![
            RequestId::String("gateway-connection-pending".to_string()),
            RequestId::String("gateway-connection-resolved".to_string()),
        ]
    );
    pretty_assertions::assert_eq!(
        cleanup.stranded_connection_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    pretty_assertions::assert_eq!(
        cleanup.resolved_notifications,
        vec![
            WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-owned".to_string(),
                    request_id: RequestId::String("gateway-thread-pending".to_string()),
                },
                method: "item/tool/requestUserInput".to_string(),
            },
            WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-owned".to_string(),
                    request_id: RequestId::String("gateway-thread-resolved".to_string()),
                },
                method: "item/tool/requestUserInput".to_string(),
            },
        ]
    );
    pretty_assertions::assert_eq!(cleanup.has_stranded_connection_scoped_requests(), true);
    pretty_assertions::assert_eq!(pending_server_requests.len(), 1);
    pretty_assertions::assert_eq!(
        pending_server_requests
            .contains_key(&RequestId::String("gateway-other-worker".to_string())),
        true
    );
    pretty_assertions::assert_eq!(resolved_server_requests.len(), 1);
    pretty_assertions::assert_eq!(
        resolved_server_requests.contains_key(&super::super::super::DownstreamServerRequestKey {
            worker_id: other_worker_id,
            request_id: RequestId::String("downstream-other-worker-resolved".to_string()),
        }),
        true
    );
}

#[test]
fn record_worker_server_request_cleanup_metrics_records_cleanup_counts() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let cleanup = WorkerServerRequestCleanup {
        resolved_thread_scoped_requests: 2,
        resolved_thread_scoped_methods: vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ],
        stranded_connection_scoped_requests: 1,
        stranded_connection_scoped_methods: vec!["account/chatgptAuthTokens/refresh".to_string()],
        ..Default::default()
    };

    record_worker_server_request_cleanup_metrics(&observability, &cleanup);

    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "worker_cleanup_resolved_thread_scoped",
                "item/tool/requestUserInput",
                2,
            ),
            (
                "worker_cleanup_stranded_connection_scoped",
                "account/chatgptAuthTokens/refresh",
                1,
            ),
        ],
    );
}

#[test]
fn record_client_server_request_cleanup_metrics_records_cleanup_counts() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);

    record_client_server_request_cleanup_metrics(
        &observability,
        &HashMap::from([
            (
                RequestId::String("gateway-thread-pending".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String(
                        "downstream-thread-pending".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-owned".to_string()),
                },
            ),
            (
                RequestId::String("gateway-connection-pending".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String(
                        "downstream-connection-pending".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: None,
                },
            ),
        ]),
        &HashMap::from([(
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(0),
                request_id: RequestId::String("downstream-resolved".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved".to_string()),
                worker_websocket_url: test_worker_websocket_url(Some(0)),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        )]),
    );

    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejected_connection_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_answered_but_unresolved",
                "item/tool/requestUserInput",
                1,
            ),
        ],
    );
}

#[tokio::test]
async fn reject_pending_server_requests_records_successful_cleanup_delivery() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let router = GatewayV2DownstreamRouter {
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
        reconnect_state: None,
    };

    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut pending_server_requests = HashMap::from([(
        RequestId::String("gateway-delivered".to_string()),
        super::super::super::PendingServerRequestRoute {
            worker_id: Some(0),
            worker_websocket_url: test_worker_websocket_url(Some(0)),
            downstream_request_id: RequestId::String("downstream-delivered".to_string()),
            method: "item/tool/requestUserInput".to_string(),
            thread_id: Some("thread-owned".to_string()),
        },
    )]);

    reject_pending_server_requests(
        &router,
        &observability,
        &request_context,
        "client_disconnected",
        Some("test cleanup"),
        &mut pending_server_requests,
        &HashMap::new(),
    )
    .await
    .expect("active downstream request handle should accept cleanup rejection");

    assert!(pending_server_requests.is_empty());
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_delivered",
                "item/tool/requestUserInput",
                1,
            ),
        ],
    );

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reject_pending_server_requests_records_failed_cleanup_delivery() {
    let (client, request_handle) = start_test_request_handle().await;
    client.shutdown().await.expect("client should shut down");

    let (event_tx, event_rx) = mpsc::channel(1);
    let router = GatewayV2DownstreamRouter {
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
        reconnect_state: Some(GatewayV2ReconnectState {
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
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-failed".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(0),
                worker_websocket_url: test_worker_websocket_url(Some(0)),
                downstream_request_id: RequestId::String("downstream-failed".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-owned".to_string()),
            },
        ),
        (
            RequestId::String("gateway-skipped".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: test_worker_websocket_url(Some(1)),
                downstream_request_id: RequestId::String("downstream-skipped".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
    ]);

    let logs = capture_logs_async(async {
        let err = reject_pending_server_requests(
            &router,
            &observability,
            &request_context,
            "client_disconnected",
            Some("test cleanup"),
            &mut pending_server_requests,
            &HashMap::new(),
        )
        .await
        .expect_err("closed downstream request handle should fail cleanup");

        pretty_assertions::assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    })
    .await;

    assert!(pending_server_requests.is_empty());
    assert!(
            logs.contains("skipping pending server-request rejection because the downstream worker route is unavailable")
        );
    assert!(logs.contains("failed to reject pending downstream server request"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-b.invalid\""));
    assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
        &metrics,
        &[
            (
                "client_cleanup_rejected_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejected_connection_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_failed",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "client_cleanup_rejection_skipped_unavailable_worker",
                "item/tool/requestUserInput",
                1,
            ),
        ],
        "item/tool/requestUserInput",
        2,
    );
}

#[test]
fn worker_cleanup_records_failed_synthesized_resolved_delivery() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let request_id = RequestId::String("gateway-srv-1".to_string());
    let err = io::Error::new(io::ErrorKind::TimedOut, "gateway websocket send timed out");

    let logs = capture_logs(|| {
        record_worker_cleanup_resolution_send_failure(
            &observability,
            &GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
            Some(2),
            "ws://worker-c.invalid",
            &WorkerCleanupResolvedNotification {
                notification: ServerRequestResolvedNotification {
                    thread_id: "thread-visible".to_string(),
                    request_id,
                },
                method: "item/tool/requestUserInput".to_string(),
            },
            &err,
        );
    });

    assert!(
        logs.contains("failed to deliver synthesized serverRequest/resolved during worker cleanup")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("worker_websocket_url=\"ws://worker-c.invalid\""));
    assert!(logs.contains("thread_id=\"thread-visible\""));
    assert!(logs.contains("gateway_request_id=String(\"gateway-srv-1\")"));
    assert!(logs.contains("method=\"item/tool/requestUserInput\""));
    assert!(logs.contains("outcome=\"client_send_timed_out\""));
    assert_worker_cleanup_resolution_send_failure_metrics(
        &metrics,
        "worker_cleanup_resolution_send_failed",
        "item/tool/requestUserInput",
        "client_send_timed_out",
    );
}
