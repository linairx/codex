use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2_connection::GatewayV2ReconnectState;
use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::northbound::v2_server_request_cleanup::record_worker_cleanup_resolution_send_failure;
use crate::northbound::v2_server_request_cleanup::reject_pending_server_requests;
use crate::northbound::v2_server_request_cleanup::should_reject_pending_server_requests_after_connection_error;
use crate::northbound::v2_server_requests::collect_server_request_cleanup_for_worker;
use crate::northbound::v2_server_requests::record_client_server_request_cleanup_metrics;
use crate::northbound::v2_server_requests::record_worker_server_request_cleanup_metrics;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::observe_v2_connection;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;

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

    assert_eq!(cleanup.resolved_thread_scoped_requests, 2);
    assert_eq!(cleanup.stranded_connection_scoped_requests, 2);
    assert_eq!(
        cleanup.resolved_thread_scoped_request_ids,
        vec![
            RequestId::String("gateway-thread-pending".to_string()),
            RequestId::String("gateway-thread-resolved".to_string()),
        ]
    );
    assert_eq!(
        cleanup.resolved_thread_scoped_thread_ids,
        vec!["thread-owned".to_string(), "thread-owned".to_string()]
    );
    assert_eq!(
        cleanup.resolved_thread_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    assert_eq!(
        cleanup.stranded_connection_scoped_request_ids,
        vec![
            RequestId::String("gateway-connection-pending".to_string()),
            RequestId::String("gateway-connection-resolved".to_string()),
        ]
    );
    assert_eq!(
        cleanup.stranded_connection_scoped_methods,
        vec![
            "item/tool/requestUserInput".to_string(),
            "item/tool/requestUserInput".to_string(),
        ]
    );
    assert_eq!(
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
    assert_eq!(cleanup.has_stranded_connection_scoped_requests(), true);
    assert_eq!(pending_server_requests.len(), 1);
    assert_eq!(
        pending_server_requests
            .contains_key(&RequestId::String("gateway-other-worker".to_string())),
        true
    );
    assert_eq!(resolved_server_requests.len(), 1);
    assert_eq!(
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

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
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

#[test]
fn aggregated_page_bounds_returns_requested_window_when_offset_is_in_range() {
    assert_eq!(
        aggregated_page_bounds(5, 1, 2, "apps-offset:"),
        (1, 3, Some("apps-offset:3".to_string()))
    );
}

#[test]
fn aggregated_page_bounds_returns_empty_page_when_offset_is_past_end() {
    assert_eq!(
        aggregated_page_bounds(2, 5, 3, "apps-offset:"),
        (2, 2, None)
    );
}

#[test]
fn connection_errors_that_end_the_northbound_socket_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidData,
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::UnexpectedEof,
        std::io::ErrorKind::WriteZero,
        std::io::ErrorKind::NotConnected,
    ] {
        let err = std::io::Error::new(kind, "test");
        assert_eq!(
            should_reject_pending_server_requests_after_connection_error(&err),
            true
        );
    }
}

#[test]
fn non_terminal_connection_errors_do_not_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidInput,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::Other,
    ] {
        let err = std::io::Error::new(kind, "test");
        assert_eq!(
            should_reject_pending_server_requests_after_connection_error(&err),
            false
        );
    }
}

#[test]
fn classify_v2_connection_error_maps_timeout_and_disconnect_kinds() {
    assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        )),
        "client_send_timed_out"
    );
    assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "closed",
        )),
        "client_disconnected"
    );
    assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid",
        )),
        "protocol_violation"
    );
    assert_eq!(
        classify_v2_connection_error(&std::io::Error::other("other")),
        "connection_error"
    );
}

#[test]
fn observe_v2_connection_records_client_send_timeout_outcome() {
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
    let context = GatewayRequestContext::default();
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();
    observability
        .v2_connection_health()
        .update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

    observe_v2_connection(
        &observability,
        connection_id,
        &context,
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "gateway websocket send timed out",
        )),
        None,
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 4,
            pending_client_request_worker_counts: Vec::new(),
            pending_client_request_method_counts: vec![
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 3,
                },
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 1,
                },
            ],
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: Vec::new(),
            server_request_backlog_method_counts: Vec::new(),
        },
        Duration::from_millis(9),
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.active_connection_count, 0);
    assert_eq!(
        health_snapshot.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(health_snapshot.last_connection_detail, None);
    assert_eq!(
        health_snapshot.last_connection_pending_client_request_count,
        4
    );
    assert_eq!(
        health_snapshot.last_connection_max_pending_client_request_count,
        6
    );
    assert_eq!(
        health_snapshot
            .last_connection_pending_client_request_started_at
            .is_some(),
        true
    );
    assert_eq!(
        health_snapshot.last_connection_pending_client_request_method_counts,
        vec![
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 3,
            },
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "thread/read".to_string(),
                pending_client_request_count: 1,
            },
        ]
    );
    assert_eq!(
        health_snapshot.last_connection_pending_server_request_count,
        2
    );
    assert_eq!(
        health_snapshot.last_connection_answered_but_unresolved_server_request_count,
        1
    );
    assert_eq!(
        health_snapshot.last_connection_server_request_backlog_count,
        3
    );
    assert_eq!(
        health_snapshot.last_connection_max_server_request_backlog_count,
        5
    );
    assert_eq!(health_snapshot.last_connection_completed_at.is_some(), true);
    assert_eq!(health_snapshot.last_connection_duration_ms, Some(9));

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_method_points = Vec::new();
    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_connections" => {
                saw_count = true;
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 9.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            "gateway_v2_connection_pending_client_requests" => {
                saw_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 4.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending client request aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending client request type"),
                }
            }
            "gateway_v2_connection_max_pending_client_requests" => {
                saw_max_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 6.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max pending client request aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max pending client request type"),
                }
            }
            "gateway_v2_connection_pending_client_requests_by_method" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        pending_client_request_method_points.extend(histogram.data_points().map(
                            |point| {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                (attributes, point.count(), point.sum())
                            },
                        ));
                    }
                    _ => panic!(
                        "unexpected v2 connection pending client request by method aggregation"
                    ),
                },
                _ => panic!("unexpected v2 connection pending client request by method type"),
            },
            "gateway_v2_connection_pending_server_requests" => {
                saw_pending_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending type"),
                }
            }
            "gateway_v2_connection_answered_but_unresolved_server_requests" => {
                saw_answered_but_unresolved_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection answered-but-unresolved aggregation"),
                    },
                    _ => panic!("unexpected v2 connection answered-but-unresolved type"),
                }
            }
            "gateway_v2_connection_server_request_backlog" => {
                saw_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection server-request backlog aggregation"),
                    },
                    _ => panic!("unexpected v2 connection server-request backlog type"),
                }
            }
            "gateway_v2_connection_max_server_request_backlog" => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 5.0);
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
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max server-request backlog aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max server-request backlog type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                3.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests);
    assert!(saw_answered_but_unresolved_server_requests);
    assert!(saw_server_request_backlog);
    assert!(saw_max_server_request_backlog);
}
