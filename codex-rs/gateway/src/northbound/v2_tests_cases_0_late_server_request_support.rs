use super::*;
use pretty_assertions::assert_eq;

pub(crate) fn assert_v2_server_request_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_points = expected
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_points.sort();

    let mut actual_points = Vec::new();
    for metric in metrics {
        if metric.name() == "gateway_v2_server_request_lifecycle_events" {
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            }
        }
    }
    actual_points.sort();
    assert_eq!(actual_points, expected_points);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_delivery_failures: &[(&str, u64)],
) {
    assert_v2_server_request_answer_account_exhaustion_metrics(
        metrics,
        expected_lifecycle,
        expected_delivery_failures,
        &[],
    );
}

pub(crate) fn assert_v2_server_request_answer_account_exhaustion_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_delivery_failures: &[(&str, u64)],
    expected_account_events: &[(usize, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();
    let mut expected_delivery_failure_points = expected_delivery_failures
        .iter()
        .map(|(response_kind, value)| {
            (
                BTreeMap::from([("response_kind".to_string(), (*response_kind).to_string())]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_delivery_failure_points.sort();
    let mut expected_account_points = expected_account_events
        .iter()
        .map(|(worker_id, event, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("worker_id".to_string(), worker_id.to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_account_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    let mut actual_delivery_failure_points = Vec::new();
    let mut actual_account_points = Vec::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_server_request_answer_delivery_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_delivery_failure_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => {
                        panic!("unexpected server-request answer delivery failure aggregation")
                    }
                },
                _ => panic!("unexpected server-request answer delivery failure count type"),
            },
            "gateway_v2_account_capacity_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_account_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected account capacity event aggregation"),
                },
                _ => panic!("unexpected account capacity event type"),
            },
            _ => {}
        }
    }
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
    actual_delivery_failure_points.sort();
    assert_eq!(
        actual_delivery_failure_points,
        expected_delivery_failure_points
    );
    actual_account_points.sort();
    assert_eq!(actual_account_points, expected_account_points);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
    expected_method: &str,
    expected_delivery_failure_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_lifecycle_points = expected_lifecycle
        .iter()
        .map(|(event, method, value)| {
            (
                BTreeMap::from([
                    ("event".to_string(), (*event).to_string()),
                    ("method".to_string(), (*method).to_string()),
                ]),
                *value,
            )
        })
        .collect::<Vec<_>>();
    expected_lifecycle_points.sort();

    let mut actual_lifecycle_points = Vec::new();
    let mut saw_delivery_failure_count = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        actual_lifecycle_points.extend(sum.data_points().map(|point| {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (attributes, point.value())
                        }));
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_server_request_rejection_delivery_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), expected_delivery_failure_count);
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
                            BTreeMap::from([("method".to_string(), expected_method.to_string(),)])
                        );
                        saw_delivery_failure_count = true;
                    }
                    _ => panic!("unexpected server-request rejection delivery failure aggregation"),
                },
                _ => panic!("unexpected server-request rejection delivery failure count type"),
            },
            _ => {}
        }
    }
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
    assert!(saw_delivery_failure_count);
}

pub(crate) fn assert_v2_server_request_lifecycle_and_connection_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_event: &str,
    expected_method: &str,
    expected_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_lifecycle_event = false;
    let mut saw_connection_count = false;
    let mut saw_connection_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
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
                                    ("event".to_string(), expected_event.to_string()),
                                    ("method".to_string(), expected_method.to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 1);
                                saw_lifecycle_event = true;
                            }
                        }
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_connections" => {
                saw_connection_count = true;
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
                                    expected_outcome.to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_connection_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("duration point");
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
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    expected_outcome.to_string(),
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

    assert!(saw_lifecycle_event);
    assert!(saw_connection_count);
    assert!(saw_connection_duration);
}
