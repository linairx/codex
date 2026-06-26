use super::*;
use pretty_assertions::assert_eq;

pub(crate) fn assert_v2_fail_closed_request_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_fail_closed_request_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_fail_closed_requests" {
            saw_fail_closed_request_count = true;
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
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected fail-closed request count aggregation"),
                },
                _ => panic!("unexpected fail-closed request count type"),
            }
        }
    }
    assert!(saw_fail_closed_request_count);
}

pub(crate) fn assert_v2_degraded_thread_discovery_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_degraded_thread_discovery_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_degraded_thread_discovery" {
            saw_degraded_thread_discovery_count = true;
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
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected degraded thread discovery count aggregation"),
                },
                _ => panic!("unexpected degraded thread discovery count type"),
            }
        }
    }
    assert!(saw_degraded_thread_discovery_count);
}

pub(crate) fn assert_v2_thread_list_deduplication_metric(
    metrics: &codex_otel::MetricsClient,
    selected_worker_id: Option<usize>,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let selected_worker_id =
        selected_worker_id.map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
    let mut saw_thread_list_deduplication_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_thread_list_deduplications" {
            saw_thread_list_deduplication_count = true;
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
                                "selected_worker_id".to_string(),
                                selected_worker_id.clone(),
                            )])
                        );
                    }
                    _ => panic!("unexpected thread-list deduplication count aggregation"),
                },
                _ => panic!("unexpected thread-list deduplication count type"),
            }
        }
    }
    assert!(saw_thread_list_deduplication_count);
}

pub(crate) fn assert_v2_thread_route_recovery_metric(
    metrics: &codex_otel::MetricsClient,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_thread_route_recovery_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_thread_route_recoveries" {
            saw_thread_route_recovery_count = true;
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
                            BTreeMap::from([("outcome".to_string(), outcome.to_string())])
                        );
                    }
                    _ => panic!("unexpected thread route recovery count aggregation"),
                },
                _ => panic!("unexpected thread route recovery count type"),
            }
        }
    }
    assert!(saw_thread_route_recovery_count);
}

pub(crate) fn assert_no_v2_metric(metrics: &codex_otel::MetricsClient, metric_name: &str) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    for metric in metrics {
        assert_ne!(metric.name(), metric_name);
    }
}

pub(crate) fn assert_v2_upstream_request_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reconnect_backoff_active: bool,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let reconnect_backoff_active = reconnect_backoff_active.to_string();
    let mut saw_upstream_request_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_upstream_request_failures" {
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
                                ("method".to_string(), method.to_string()),
                                (
                                    "reconnect_backoff_active".to_string(),
                                    reconnect_backoff_active.clone(),
                                ),
                            ])
                        );
                    }
                    _ => panic!("unexpected upstream request failure count aggregation"),
                },
                _ => panic!("unexpected upstream request failure count type"),
            }
        }
    }
    assert!(saw_upstream_request_failure_count);
}

pub(crate) fn assert_v2_suppressed_notification_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    reason: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_suppressed_notification_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_suppressed_notifications" {
            saw_suppressed_notification_count = true;
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
                                ("method".to_string(), method.to_string()),
                                ("reason".to_string(), reason.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected suppressed notification count aggregation"),
                },
                _ => panic!("unexpected suppressed notification count type"),
            }
        }
    }
    assert!(saw_suppressed_notification_count);
}

pub(crate) fn assert_no_v2_suppressed_notification_metric(metrics: &codex_otel::MetricsClient) {
    assert_no_v2_metric(metrics, "gateway_v2_suppressed_notifications");
}

pub(crate) fn assert_v2_forwarded_notification_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    expected_count: u64,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_forwarded_notification_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_forwarded_notifications" {
            saw_forwarded_notification_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), expected_count);
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
                            BTreeMap::from([("method".to_string(), method.to_string())])
                        );
                    }
                    _ => panic!("unexpected forwarded notification count aggregation"),
                },
                _ => panic!("unexpected forwarded notification count type"),
            }
        }
    }
    assert!(saw_forwarded_notification_count);
}

pub(crate) fn assert_no_v2_forwarded_notification_metric(metrics: &codex_otel::MetricsClient) {
    assert_no_v2_metric(metrics, "gateway_v2_forwarded_notifications");
}

pub(crate) fn assert_v2_server_request_rejection_and_lifecycle_metrics(
    metrics: &codex_otel::MetricsClient,
    rejection_method: &str,
    rejection_reason: &str,
    expected_lifecycle: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_server_request_rejection_count = false;
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
            "gateway_v2_server_request_rejections" => {
                saw_server_request_rejection_count = true;
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
                                    ("method".to_string(), rejection_method.to_string()),
                                    ("reason".to_string(), rejection_reason.to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected server-request rejection count aggregation"),
                    },
                    _ => panic!("unexpected server-request rejection count type"),
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
    assert!(saw_server_request_rejection_count);
    actual_lifecycle_points.sort();
    assert_eq!(actual_lifecycle_points, expected_lifecycle_points);
}

pub(crate) fn assert_command_exec_pending_client_limit_metrics(
    metrics: &codex_otel::MetricsClient,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    let mut saw_client_request_rejection_count = false;

    for metric in metrics {
        match metric.name() {
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
            "gateway_v2_client_request_rejections" => {
                saw_client_request_rejection_count = true;
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
                                    ("method".to_string(), "command/exec".to_string()),
                                    ("reason".to_string(), "pending_limit".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected client-request rejection count aggregation"),
                    },
                    _ => panic!("unexpected client-request rejection count type"),
                }
            }
            _ => {}
        }
    }

    let expected = [
        ("initialize", "ok", 1),
        ("command/exec", "rate_limited", 1),
        ("command/exec", "ok", 1),
    ];
    for (method, outcome, count) in expected {
        assert_eq!(
            observed_request_counts.get(&(method.to_string(), outcome.to_string())),
            Some(&count),
            "missing v2 request count metric for method={method} outcome={outcome}"
        );
        assert_eq!(
            observed_request_durations.get(&(method.to_string(), outcome.to_string())),
            Some(&count),
            "missing v2 request duration metric for method={method} outcome={outcome}"
        );
    }
    assert!(saw_client_request_rejection_count);
}

pub(crate) fn request_metric_point_tags(attributes: BTreeMap<String, String>) -> (String, String) {
    let method = attributes
        .get("method")
        .expect("request metric should have method")
        .clone();
    let outcome = attributes
        .get("outcome")
        .expect("request metric should have outcome")
        .clone();
    (method, outcome)
}

pub(crate) fn assert_v2_request_metrics(
    metrics: &codex_otel::MetricsClient,
    expected: &[(&str, &str, u64)],
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_counts = BTreeMap::new();
    let mut observed_durations = BTreeMap::new();
    let mut saw_request_count = false;
    let mut saw_request_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_requests" => {
                saw_request_count = true;
                match metric.data() {
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
                                *observed_counts.entry((method, outcome)).or_insert(0) +=
                                    point.value();
                            }
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
                                *observed_durations.entry((method, outcome)).or_insert(0) +=
                                    point.count();
                            }
                        }
                        _ => panic!("unexpected v2 request duration aggregation"),
                    },
                    _ => panic!("unexpected v2 request duration type"),
                }
            }
            _ => {}
        }
    }
    assert!(saw_request_count);
    assert!(saw_request_duration);
    assert_eq!(
        observed_counts.values().copied().sum::<u64>(),
        expected.iter().map(|(_, _, count)| *count).sum::<u64>()
    );
    assert_eq!(
        observed_durations.values().copied().sum::<u64>(),
        expected.iter().map(|(_, _, count)| *count).sum::<u64>()
    );
    for (method, outcome, count) in expected {
        assert_eq!(
            observed_counts.get(&(method.to_string(), outcome.to_string())),
            Some(count),
            "missing v2 request count metric for method={method} outcome={outcome}"
        );
        assert_eq!(
            observed_durations.get(&(method.to_string(), outcome.to_string())),
            Some(count),
            "missing v2 request duration metric for method={method} outcome={outcome}"
        );
    }
}

pub(crate) fn assert_repeated_initialize_metrics(metrics: &codex_otel::MetricsClient) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut observed_request_counts = BTreeMap::new();
    let mut observed_request_durations = BTreeMap::new();
    let mut saw_protocol_violation_count = false;

    for metric in metrics {
        match metric.name() {
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
            "gateway_v2_protocol_violations" => match metric.data() {
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
                                ("phase".to_string(), "post_initialize".to_string()),
                                ("reason".to_string(), "repeated_initialize".to_string()),
                            ])
                        );
                        saw_protocol_violation_count = true;
                    }
                    _ => panic!("unexpected protocol violation count aggregation"),
                },
                _ => panic!("unexpected protocol violation count type"),
            },
            _ => {}
        }
    }

    let expected_request_metrics = BTreeMap::from([
        (("initialize".to_string(), "ok".to_string()), 1),
        (("initialize".to_string(), "invalid_request".to_string()), 1),
    ]);
    assert_eq!(observed_request_counts, expected_request_metrics);
    assert_eq!(observed_request_durations, expected_request_metrics);
    assert!(saw_protocol_violation_count);
}

pub(crate) fn assert_v2_notification_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_notification_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_notification_send_failures" {
            saw_notification_send_failure_count = true;
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
                                ("method".to_string(), method.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            }
        }
    }
    assert!(saw_notification_send_failure_count);
}

pub(crate) fn assert_v2_client_response_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    method: &str,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_response_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_client_response_send_failures" {
            saw_response_send_failure_count = true;
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
                                ("method".to_string(), method.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected client response send failure count aggregation"),
                },
                _ => panic!("unexpected client response send failure count type"),
            }
        }
    }
    assert!(saw_response_send_failure_count);
}

pub(crate) fn assert_v2_close_frame_send_failure_metric(
    metrics: &codex_otel::MetricsClient,
    code: u16,
    outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_close_frame_send_failure_count = false;
    for metric in metrics {
        if metric.name() == "gateway_v2_close_frame_send_failures" {
            saw_close_frame_send_failure_count = true;
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
                                ("code".to_string(), code.to_string()),
                                ("outcome".to_string(), outcome.to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected close frame send failure count aggregation"),
                },
                _ => panic!("unexpected close frame send failure count type"),
            }
        }
    }
    assert!(saw_close_frame_send_failure_count);
}

pub(crate) fn assert_worker_cleanup_resolution_send_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    lifecycle_event: &str,
    lifecycle_method: &str,
    notification_outcome: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_lifecycle_count = false;
    let mut saw_notification_send_failure_count = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("lifecycle point");
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
                                ("event".to_string(), lifecycle_event.to_string()),
                                ("method".to_string(), lifecycle_method.to_string()),
                            ])
                        );
                        saw_lifecycle_count = true;
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_notification_send_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum
                            .data_points()
                            .next()
                            .expect("notification failure point");
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
                                ("method".to_string(), "serverRequest/resolved".to_string()),
                                ("outcome".to_string(), notification_outcome.to_string()),
                            ])
                        );
                        saw_notification_send_failure_count = true;
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            },
            _ => {}
        }
    }

    assert!(saw_lifecycle_count);
    assert!(saw_notification_send_failure_count);
}
