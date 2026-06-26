use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_forwarded_notification_metrics_with_method_tags() {
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

    observability.record_v2_forwarded_notification("configWarning");

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
    for metric in metrics {
        if metric.name() == V2_FORWARDED_NOTIFICATION_COUNT_METRIC {
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
                            BTreeMap::from([("method".to_string(), "configWarning".to_string()),])
                        );
                    }
                    _ => panic!("unexpected forwarded notification count aggregation"),
                },
                _ => panic!("unexpected forwarded notification count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.forwarded_notification_counts,
        vec![GatewayV2ForwardedNotificationCounts {
            method: "configWarning".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_forwarded_notification_method,
        Some("configWarning".to_string())
    );
    assert_eq!(health.last_forwarded_notification_at.is_some(), true);
}

#[test]
fn records_v2_notification_send_failure_metrics_with_method_and_outcome_tags() {
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

    observability.record_v2_notification_send_failure("warning", "client_send_timed_out");

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
    for metric in metrics {
        if metric.name() == V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC {
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
                            BTreeMap::from([
                                ("method".to_string(), "warning".to_string()),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_notification_send_failure_method,
        Some("warning".to_string())
    );
    assert_eq!(
        health.last_notification_send_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(health.last_notification_send_failure_at.is_some(), true);
}

#[test]
fn records_v2_suppressed_notification_metrics_with_reason_tags() {
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

    observability.record_v2_suppressed_notification("skills/changed", "pending_refresh");

    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.suppressed_notification_counts.len(), 1);
    assert_eq!(
        health.suppressed_notification_counts[0].method,
        "skills/changed"
    );
    assert_eq!(
        health.suppressed_notification_counts[0].reason,
        "pending_refresh"
    );
    assert_eq!(health.suppressed_notification_counts[0].count, 1);
    assert_eq!(
        health.last_suppressed_notification_method,
        Some("skills/changed".to_string())
    );
    assert_eq!(
        health.last_suppressed_notification_reason,
        Some("pending_refresh".to_string())
    );
    assert_eq!(health.last_suppressed_notification_at.is_some(), true);

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
    for metric in metrics {
        if metric.name() == V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC {
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
                            BTreeMap::from([
                                ("method".to_string(), "skills/changed".to_string()),
                                ("reason".to_string(), "pending_refresh".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected suppressed notification count aggregation"),
                },
                _ => panic!("unexpected suppressed notification count type"),
            }
        }
    }

    assert!(saw_count);
}
