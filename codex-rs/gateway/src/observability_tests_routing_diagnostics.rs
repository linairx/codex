use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_thread_list_deduplication_metrics_with_selected_worker_tag() {
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

    observability.record_v2_thread_list_deduplication(Some(4));

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
        if metric.name() == V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC {
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
                            BTreeMap::from([("selected_worker_id".to_string(), "4".to_string(),)])
                        );
                    }
                    _ => panic!("unexpected thread-list deduplication count aggregation"),
                },
                _ => panic!("unexpected thread-list deduplication count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.thread_list_deduplication_counts,
        vec![GatewayV2ThreadListDeduplicationCounts {
            selected_worker_id: Some(4),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_thread_list_deduplication_selected_worker_id,
        Some(4)
    );
    assert_eq!(health.last_thread_list_deduplication_at.is_some(), true);
}

#[test]
fn records_v2_thread_route_recovery_metrics_with_outcome_tag() {
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

    observability.record_v2_thread_route_recovery("miss");

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
        if metric.name() == V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC {
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
                            BTreeMap::from([("outcome".to_string(), "miss".to_string())])
                        );
                    }
                    _ => panic!("unexpected thread route recovery count aggregation"),
                },
                _ => panic!("unexpected thread route recovery count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.thread_route_recovery_counts,
        vec![GatewayV2ThreadRouteRecoveryCounts {
            outcome: "miss".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_thread_route_recovery_outcome,
        Some("miss".to_string())
    );
    assert_eq!(health.last_thread_route_recovery_at.is_some(), true);
}

#[test]
fn records_v2_degraded_thread_discovery_metrics_with_method_and_backoff_tags() {
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

    observability.record_v2_degraded_thread_discovery("thread/list", true);

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
        if metric.name() == V2_DEGRADED_THREAD_DISCOVERY_COUNT_METRIC {
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
                                ("method".to_string(), "thread/list".to_string()),
                                ("reconnect_backoff_active".to_string(), "true".to_string(),),
                            ])
                        );
                    }
                    _ => panic!("unexpected degraded thread discovery count aggregation"),
                },
                _ => panic!("unexpected degraded thread discovery count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.degraded_thread_discovery_counts,
        vec![GatewayV2DegradedThreadDiscoveryCounts {
            method: "thread/list".to_string(),
            reconnect_backoff_active: true,
            count: 1,
        }]
    );
    assert_eq!(
        health.last_degraded_thread_discovery_method,
        Some("thread/list".to_string())
    );
    assert_eq!(
        health.last_degraded_thread_discovery_reconnect_backoff_active,
        Some(true)
    );
    assert_eq!(health.last_degraded_thread_discovery_at.is_some(), true);
}
