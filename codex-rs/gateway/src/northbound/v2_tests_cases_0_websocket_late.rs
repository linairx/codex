use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_emits_v2_request_metrics() {
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
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: Some(0),
            turn_start_quota_per_minute: None,
        }),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("model-list".to_string()),
                method: "model/list".to_string(),
                params: Some(serde_json::json!({})),
                trace: None,
            }))
            .expect("request should serialize")
            .into(),
        ))
        .await
        .expect("model list should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("expected model list error response");
    };
    assert_eq!(error.id, RequestId::String("model-list".to_string()));

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 2);
                            let mut saw_initialize = false;
                            let mut saw_model_list = false;
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
                                        ("method".to_string(), "initialize".to_string()),
                                        ("outcome".to_string(), "ok".to_string()),
                                    ])
                                {
                                    saw_initialize = true;
                                }
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "model/list".to_string()),
                                        ("outcome".to_string(), "rate_limited".to_string()),
                                    ])
                                {
                                    saw_model_list = true;
                                }
                            }
                            assert_eq!(saw_initialize, true);
                            assert_eq!(saw_model_list, true);
                        }
                        _ => panic!("unexpected v2 count aggregation"),
                    },
                    _ => panic!("unexpected v2 count type"),
                }
            }
            "gateway_v2_request_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 2);
                            let mut saw_initialize = false;
                            let mut saw_model_list = false;
                            for point in histogram.data_points() {
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
                                        ("method".to_string(), "initialize".to_string()),
                                        ("outcome".to_string(), "ok".to_string()),
                                    ])
                                {
                                    saw_initialize = true;
                                }
                                if attributes
                                    == BTreeMap::from([
                                        ("method".to_string(), "model/list".to_string()),
                                        ("outcome".to_string(), "rate_limited".to_string()),
                                    ])
                                {
                                    saw_model_list = true;
                                }
                            }
                            assert_eq!(saw_initialize, true);
                            assert_eq!(saw_model_list, true);
                        }
                        _ => panic!("unexpected v2 duration aggregation"),
                    },
                    _ => panic!("unexpected v2 duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_emits_v2_connection_metrics_for_initialize_timeout() {
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
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: start_mock_remote_server_for_initialize().await,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            test_initialize_response().await,
        ))),
        timeouts: GatewayV2Timeouts {
            initialize: Duration::from_millis(50),
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let (_websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_request_count = false;
    let mut saw_request_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_request_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("request count point");
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
                                request_metric_point_tags(attributes),
                                ("initialize".to_string(), "timed_out".to_string())
                            );
                        }
                        _ => panic!("unexpected v2 request count aggregation"),
                    },
                    _ => panic!("unexpected v2 request count type"),
                }
            }
            "gateway_v2_request_duration" => {
                saw_request_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram
                                .data_points()
                                .next()
                                .expect("request duration point");
                            assert_eq!(point.count(), 1);
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
                                request_metric_point_tags(attributes),
                                ("initialize".to_string(), "timed_out".to_string())
                            );
                        }
                        _ => panic!("unexpected v2 request duration aggregation"),
                    },
                    _ => panic!("unexpected v2 request duration type"),
                }
            }
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 1);
                            let point = sum.data_points().next().expect("count point");
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
                                    "initialize_timed_out".to_string(),
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
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 1);
                            let point = histogram.data_points().next().expect("histogram point");
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
                                    "initialize_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);
    assert_eq!(saw_request_count, true);
    assert_eq!(saw_request_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_emits_v2_connection_metrics_for_downstream_disconnect() {
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
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_that_disconnects_after_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    let _ = wait_for_close_frame(&mut websocket).await;

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let total: u64 = sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum();
                            assert_eq!(total, 1);
                            let point = sum.data_points().next().expect("count point");
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
                                    "downstream_session_ended".to_string(),
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
                            let total_count: u64 = histogram
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum();
                            assert_eq!(total_count, 1);
                            let point = histogram.data_points().next().expect("histogram point");
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            _ => {}
        }
    }

    assert_eq!(saw_count, true);
    assert_eq!(saw_duration, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_with_reason_when_downstream_disconnects() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_that_disconnects_after_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::ERROR
    );
    assert_eq!(
        close_frame
            .reason
            .starts_with("downstream app-server disconnected:"),
        true
    );

    server_task.abort();
    let _ = server_task.await;
}
