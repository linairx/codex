use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2_connection::GatewayV2ReconnectState;

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_success_metrics() {
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
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
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
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
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
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 2);
    assert_v2_worker_reconnect_metrics(&metrics, &[(1, "attempt"), (1, "success")]);

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_reuses_initialize_capabilities() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let recorded_initialize_params = Arc::new(Mutex::new(Vec::new()));
    let worker_b =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec![
                "thread/started".to_string(),
                "item/agentMessage/delta".to_string(),
            ]),
        }),
    };
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
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
            ),
            initialize_params: initialize_params.clone(),
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
        .await;

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        *recorded_initialize_params.lock().await,
        vec![initialize_params]
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_connect_failure_metrics() {
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
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
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
                "ws://127.0.0.1:1".to_string(),
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
                            websocket_url: "ws://127.0.0.1:1".to_string(),
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
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 1);
    assert_v2_worker_reconnect_metrics(
        &metrics,
        &[
            (1, "attempt"),
            (1, "attempt"),
            (1, "attempt"),
            (1, "connect_failure"),
            (1, "connect_failure"),
            (1, "connect_failure"),
        ],
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_downstream_protocol_violation_metrics() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_that_sends_invalid_jsonrpc_during_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
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
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
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
            retry_backoff: Duration::from_secs(3),
        }),
    };

    let logs = capture_logs_async(async {
        router
            .reconnect_missing_workers_at(Instant::now(), &observability, true)
            .await;
        assert_eq!(router.worker_count(), 1);
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("sent invalid initialize response"), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut saw_protocol_violation = false;
    let mut saw_worker_reconnect_attempt = false;
    let mut saw_worker_reconnect_failure = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if attributes
                                == BTreeMap::from([
                                    ("phase".to_string(), "downstream".to_string()),
                                    ("reason".to_string(), "invalid_jsonrpc".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 1);
                                saw_protocol_violation = true;
                            }
                        }
                    }
                    _ => panic!("unexpected protocol violation count aggregation"),
                },
                _ => panic!("unexpected protocol violation count type"),
            },
            "gateway_v2_worker_reconnects" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if attributes
                                == BTreeMap::from([
                                    ("worker_id".to_string(), "1".to_string()),
                                    ("outcome".to_string(), "attempt".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 3);
                                saw_worker_reconnect_attempt = true;
                            }
                            if attributes
                                == BTreeMap::from([
                                    ("worker_id".to_string(), "1".to_string()),
                                    ("outcome".to_string(), "connect_failure".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 3);
                                saw_worker_reconnect_failure = true;
                            }
                        }
                    }
                    _ => panic!("unexpected worker reconnect count aggregation"),
                },
                _ => panic!("unexpected worker reconnect count type"),
            },
            _ => {}
        }
    }
    assert!(saw_protocol_violation);
    assert!(saw_worker_reconnect_attempt);
    assert!(saw_worker_reconnect_failure);

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_replay_failure_metrics() {
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
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let replay_failure_worker = start_mock_remote_server_that_disconnects_after_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                replay_failure_worker.clone(),
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
                            websocket_url: replay_failure_worker,
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
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 1);
    assert_v2_worker_reconnect_metrics(
        &metrics,
        &[
            (1, "attempt"),
            (1, "attempt"),
            (1, "attempt"),
            (1, "replay_failure"),
        ],
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[test]
fn worker_reconnect_backoff_suppresses_immediate_retry_attempts() {
    let (event_tx, event_rx) = mpsc::channel(1);
    let now = Instant::now();
    let retry_backoff = Duration::from_secs(3);
    let mut router = GatewayV2DownstreamRouter {
        workers: Vec::new(),
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

    assert!(router.should_attempt_worker_reconnect(0, now));
    assert!(router.should_attempt_worker_reconnect(1, now));

    router.record_worker_reconnect_failure(0, now, retry_backoff);
    assert!(!router.should_attempt_worker_reconnect(0, now));
    assert!(!router.should_attempt_worker_reconnect(0, now + Duration::from_secs(2)));
    assert!(router.should_attempt_worker_reconnect(0, now + retry_backoff));
    assert!(router.should_attempt_worker_reconnect(1, now));

    router.clear_worker_reconnect_failure(0);
    assert!(router.should_attempt_worker_reconnect(0, now + Duration::from_secs(1)));
}

#[tokio::test]
async fn reconnect_missing_workers_records_backoff_suppressed_metric() {
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
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let now = Instant::now();
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
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
            retry_backoff: Duration::from_secs(3),
        }),
    };
    router.record_worker_reconnect_failure(1, now, Duration::from_secs(3));

    let logs = capture_logs_async(async {
        router
            .reconnect_missing_workers_at(now, &observability, true)
            .await;

        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert_v2_worker_reconnect_metric(&metrics, 1, "backoff_suppressed");
    assert!(
        logs.contains(
            "suppressing missing downstream worker reconnect while retry backoff is active"
        )
    );
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("initialized_notification_sent=false"));
    assert!(logs.contains("active_fs_watch_count=0"));
    assert!(logs.contains("retry_backoff_seconds=3"));
    assert!(logs.contains("reconnect_backoff_remaining_seconds="));
    client.shutdown().await.expect("client should shut down");
}
