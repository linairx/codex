use super::*;
use pretty_assertions::assert_eq;

pub(crate) fn assert_v2_protocol_violation_and_connection_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut saw_connection_count = false;
    let mut saw_connection_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
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
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
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
                                BTreeMap::from([("outcome".to_string(), outcome.to_string(),)])
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
                                BTreeMap::from([("outcome".to_string(), outcome.to_string(),)])
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

    assert!(saw_protocol_violation_count);
    assert!(saw_connection_count);
    assert!(saw_connection_duration);
}

pub(crate) fn assert_v2_protocol_violation_and_request_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    request_method: &str,
    request_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
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
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            _ => {}
        }
    }

    assert!(saw_protocol_violation_count);
    let expected_request = (request_method.to_string(), request_outcome.to_string());
    assert_eq!(observed_request_counts.get(&expected_request), Some(&1));
    assert_eq!(observed_request_durations.get(&expected_request), Some(&1));
}

pub(crate) fn assert_v2_protocol_violation_connection_and_request_metrics(
    metrics: &codex_otel::MetricsClient,
    phase: &str,
    reason: &str,
    connection_outcome: &str,
    request_method: &str,
    request_outcome: &str,
    expected_request_metric_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_protocol_violation_count = false;
    let mut observed_connection_counts = BTreeMap::new();
    let mut observed_connection_durations = BTreeMap::new();
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => {
                saw_protocol_violation_count = true;
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
                                    ("phase".to_string(), phase.to_string()),
                                    ("reason".to_string(), reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected protocol violation count aggregation"),
                    },
                    _ => panic!("unexpected protocol violation count type"),
                }
            }
            "gateway_v2_connections" => match metric.data() {
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
                            let outcome = attributes
                                .get("outcome")
                                .expect("connection metric should have outcome")
                                .clone();
                            *observed_connection_counts.entry(outcome).or_insert(0) +=
                                point.value();
                        }
                    }
                    _ => panic!("unexpected v2 connection count aggregation"),
                },
                _ => panic!("unexpected v2 connection count type"),
            },
            "gateway_v2_connection_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
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
                            let outcome = attributes
                                .get("outcome")
                                .expect("connection metric should have outcome")
                                .clone();
                            *observed_connection_durations.entry(outcome).or_insert(0) +=
                                point.count();
                        }
                    }
                    _ => panic!("unexpected v2 connection duration aggregation"),
                },
                _ => panic!("unexpected v2 connection duration type"),
            },
            "gateway_v2_requests" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_counts
                                .entry((method, outcome))
                                .or_insert(0) += point.value();
                        }
                    }
                    _ => panic!("unexpected v2 request count aggregation"),
                },
                _ => panic!("unexpected v2 request count type"),
            },
            "gateway_v2_request_duration" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        for point in histogram.data_points() {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let (method, outcome) = request_metric_point_tags(attributes);
                            *observed_request_durations
                                .entry((method, outcome))
                                .or_insert(0) += point.count();
                        }
                    }
                    _ => panic!("unexpected v2 request duration aggregation"),
                },
                _ => panic!("unexpected v2 request duration type"),
            },
            _ => {}
        }
    }

    assert!(saw_protocol_violation_count);
    assert_eq!(
        observed_connection_counts.get(connection_outcome),
        Some(&1),
        "missing v2 connection count metric for outcome={connection_outcome}"
    );
    assert_eq!(
        observed_connection_durations.get(connection_outcome),
        Some(&1),
        "missing v2 connection duration metric for outcome={connection_outcome}"
    );
    let expected_request = (request_method.to_string(), request_outcome.to_string());
    assert_eq!(
        observed_request_counts.get(&expected_request),
        Some(&expected_request_metric_count),
        "missing v2 request count metric for method={request_method} outcome={request_outcome}"
    );
    assert_eq!(
        observed_request_durations.get(&expected_request),
        Some(&expected_request_metric_count),
        "missing v2 request duration metric for method={request_method} outcome={request_outcome}"
    );
}

pub(crate) fn assert_v2_downstream_backpressure_metric(
    metrics: &codex_otel::MetricsClient,
    worker_id: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_backpressure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_downstream_backpressure_events" {
            saw_backpressure_count = true;
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
                            BTreeMap::from([("worker_id".to_string(), worker_id.to_string()),])
                        );
                    }
                    _ => panic!("unexpected downstream backpressure count aggregation"),
                },
                _ => panic!("unexpected downstream backpressure count type"),
            }
        }
    }
    assert!(saw_backpressure_count);
}

pub(crate) fn assert_v2_client_send_timeout_and_server_request_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    expected_lifecycle: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_client_send_timeout_count = false;
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
    for metric in metrics {
        match metric.name() {
            "gateway_v2_client_send_timeouts" => {
                saw_client_send_timeout_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            assert_eq!(point.value(), 1);
                        }
                        _ => panic!("unexpected client send timeout count aggregation"),
                    },
                    _ => panic!("unexpected client send timeout count type"),
                }
            }
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
            _ => {}
        }
    }

    assert!(saw_client_send_timeout_count);
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
}

pub(crate) fn assert_v2_worker_reconnect_metric(
    metrics: &codex_otel::MetricsClient,
    worker_id: usize,
    outcome: &str,
) {
    assert_v2_worker_reconnect_metrics(metrics, &[(worker_id, outcome)]);
}

pub(crate) fn assert_v2_account_capacity_event_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(usize, &str)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_attributes = expected
        .iter()
        .map(|(worker_id, event)| {
            (
                BTreeMap::from([
                    ("worker_id".to_string(), worker_id.to_string()),
                    ("event".to_string(), (*event).to_string()),
                ]),
                false,
            )
        })
        .collect::<Vec<_>>();
    for metric in metrics {
        if metric.name() == "gateway_v2_account_capacity_events" {
            match metric.data() {
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
                            if let Some((_, seen)) = expected_attributes
                                .iter_mut()
                                .find(|(expected, _)| *expected == attributes)
                            {
                                *seen = true;
                                assert_eq!(point.value(), 1);
                            }
                        }
                    }
                    _ => panic!("unexpected account capacity event aggregation"),
                },
                _ => panic!("unexpected account capacity event type"),
            }
        }
    }

    let missing = expected_attributes
        .into_iter()
        .filter_map(|(attributes, seen)| (!seen).then_some(attributes))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing gateway_v2_account_capacity_events metric points: {missing:?}"
    );
}

pub(crate) fn assert_v2_account_capacity_event_metric_count(
    metrics: &codex_otel::MetricsClient,
    worker_id: usize,
    event: &str,
    expected_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    for metric in metrics {
        if metric.name() != "gateway_v2_account_capacity_events" {
            continue;
        }
        match metric.data() {
            AggregatedMetrics::U64(MetricData::Sum(sum)) => {
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
                            ("worker_id".to_string(), worker_id.to_string()),
                            ("event".to_string(), event.to_string()),
                        ])
                    {
                        assert_eq!(point.value(), expected_count);
                        return;
                    }
                }
            }
            AggregatedMetrics::U64(_) => {
                panic!("unexpected account capacity event aggregation")
            }
            _ => panic!("unexpected account capacity event type"),
        }
    }

    panic!(
        "missing gateway_v2_account_capacity_events metric point for worker {worker_id} and event {event}"
    );
}

pub(crate) fn assert_v2_account_capacity_event_health(
    observability: &GatewayObservability,
    worker_id: usize,
    event: &str,
    count: usize,
    tenant_id: &str,
    project_id: Option<&str>,
    reason: &str,
) {
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [(event.to_string(), count)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(worker_id, [(event.to_string(), count)].into())]
    );
    assert_eq!(health.last_account_capacity_event.as_deref(), Some(event));
    assert_eq!(
        health.last_account_capacity_event_worker_id,
        Some(worker_id)
    );
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some(tenant_id)
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        project_id
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(reason)
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
}

pub(crate) fn assert_v2_worker_reconnect_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(usize, &str)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut expected_attributes = BTreeMap::new();
    for (worker_id, outcome) in expected {
        let (count, _) = expected_attributes
            .entry(BTreeMap::from([
                ("worker_id".to_string(), worker_id.to_string()),
                ("outcome".to_string(), (*outcome).to_string()),
            ]))
            .or_insert((0_u64, false));
        *count += 1;
    }
    for metric in metrics {
        if metric.name() == "gateway_v2_worker_reconnects" {
            match metric.data() {
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
                            if let Some((expected_count, seen)) =
                                expected_attributes.get_mut(&attributes)
                            {
                                *seen = true;
                                assert_eq!(point.value(), *expected_count);
                            }
                        }
                    }
                    _ => panic!("unexpected worker reconnect count aggregation"),
                },
                _ => panic!("unexpected worker reconnect count type"),
            }
        }
    }
    let missing = expected_attributes
        .into_iter()
        .filter_map(|(attributes, (_, seen))| (!seen).then_some(attributes))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing gateway_v2_worker_reconnects metric points: {missing:?}"
    );
}
