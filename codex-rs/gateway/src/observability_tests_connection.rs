use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_connection_metrics_with_outcome_tags() {
    let exporter = InMemoryMetricExporter::default();
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            exporter,
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics), true);

    observability.record_v2_connection(
        "downstream_session_ended",
        Duration::from_millis(14),
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 3,
            pending_client_request_worker_counts: vec![
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(2),
                    pending_client_request_count: 2,
                },
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(7),
                    pending_client_request_count: 1,
                },
            ],
            pending_client_request_method_counts: vec![
                GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 2,
                },
                GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 1,
                },
            ],
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(3),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 2,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(8),
                    pending_server_request_count: 0,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 1,
                },
            ],
            server_request_backlog_method_counts: vec![GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 3,
            }],
        },
        5,
        7,
    );

    let resource_metrics = observability
        .metrics
        .as_ref()
        .expect("metrics client")
        .snapshot()
        .expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_worker_points = Vec::new();
    let mut pending_client_request_method_points = Vec::new();
    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut pending_server_request_worker_points = Vec::new();
    let mut answered_but_unresolved_server_request_worker_points = Vec::new();
    let mut server_request_backlog_worker_points = Vec::new();
    let mut saw_pending_server_requests_by_method = false;
    let mut saw_answered_but_unresolved_server_requests_by_method = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    let mut saw_server_request_backlog_by_method = false;
    for metric in metrics {
        match metric.name() {
            name if name == V2_CONNECTION_COUNT_METRIC => {
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected connection count aggregation"),
                    },
                    _ => panic!("unexpected connection count type"),
                }
            }
            name if name == V2_CONNECTION_DURATION_METRIC => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 14.0);
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
                        _ => panic!("unexpected connection duration aggregation"),
                    },
                    _ => panic!("unexpected connection duration type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC => {
                saw_pending_client_requests = true;
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected pending client request aggregation"),
                    },
                    _ => panic!("unexpected pending client request type"),
                }
            }
            name if name == V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC => {
                saw_max_pending_client_requests = true;
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected max pending client request aggregation"),
                    },
                    _ => panic!("unexpected max pending client request type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_method_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending client request by method aggregation")
                        }
                    },
                    _ => panic!("unexpected pending client request by method type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending client request by worker aggregation")
                        }
                    },
                    _ => panic!("unexpected pending client request by worker type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC => {
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected pending server request aggregation"),
                    },
                    _ => panic!("unexpected pending server request type"),
                }
            }
            name if name == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC => {
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => {
                            panic!("unexpected answered-but-unresolved server request aggregation")
                        }
                    },
                    _ => panic!("unexpected answered-but-unresolved server request type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC => {
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
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected server request backlog aggregation"),
                    },
                    _ => panic!("unexpected server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 7.0);
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
                        _ => panic!("unexpected max server request backlog aggregation"),
                    },
                    _ => panic!("unexpected max server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending server request by worker aggregation")
                        }
                    },
                    _ => panic!("unexpected pending server request by worker type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC =>
            {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            answered_but_unresolved_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by worker aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by worker type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            server_request_backlog_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => panic!("unexpected server request backlog by worker aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by worker type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC => {
                saw_pending_server_requests_by_method = true;
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
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected pending server request by method aggregation"),
                    },
                    _ => panic!("unexpected pending server request by method type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC =>
            {
                saw_answered_but_unresolved_server_requests_by_method = true;
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
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by method aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by method type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC => {
                saw_server_request_backlog_by_method = true;
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
                                BTreeMap::from([
                                    (
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected server request backlog by method aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by method type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "2".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "7".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string(),),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string(),),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
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
    pending_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                0.0,
            ),
        ]
    );
    answered_but_unresolved_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        answered_but_unresolved_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                0.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    server_request_backlog_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        server_request_backlog_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests_by_method);
    assert!(saw_answered_but_unresolved_server_requests_by_method);
    assert!(saw_server_request_backlog_by_method);
}
