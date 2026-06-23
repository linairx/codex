use super::*;
use pretty_assertions::assert_eq;

pub(crate) fn records_request_metrics_with_route_and_status_tags() {
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

    observability.record_http_request("POST", "/v1/threads", 200, Duration::from_millis(12));

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
    for metric in metrics {
        match metric.name() {
            REQUEST_COUNT_METRIC => {
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
                                    ("method".to_string(), "POST".to_string()),
                                    ("route".to_string(), "/v1/threads".to_string()),
                                    ("status".to_string(), "200".to_string()),
                                    ("status_class".to_string(), "2xx".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected request count aggregation"),
                    },
                    _ => panic!("unexpected request count type"),
                }
            }
            REQUEST_DURATION_METRIC => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 12.0);
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
                                    ("method".to_string(), "POST".to_string()),
                                    ("route".to_string(), "/v1/threads".to_string()),
                                    ("status".to_string(), "200".to_string()),
                                    ("status_class".to_string(), "2xx".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected duration aggregation"),
                    },
                    _ => panic!("unexpected duration type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert_eq!(saw_duration, true);
}

pub(crate) fn records_v2_request_metrics_with_method_and_outcome_tags() {
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

    observability.record_v2_request("thread/start", "ok", Duration::from_millis(8));

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
    for metric in metrics {
        match metric.name() {
            V2_REQUEST_COUNT_METRIC => {
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
                                    ("method".to_string(), "thread/start".to_string()),
                                    ("outcome".to_string(), "ok".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected request count aggregation"),
                    },
                    _ => panic!("unexpected request count type"),
                }
            }
            V2_REQUEST_DURATION_METRIC => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 8.0);
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
                                    ("method".to_string(), "thread/start".to_string()),
                                    ("outcome".to_string(), "ok".to_string()),
                                ])
                            );
                        }
                        _ => panic!("unexpected duration aggregation"),
                    },
                    _ => panic!("unexpected duration type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert_eq!(saw_duration, true);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.request_counts.len(), 1);
    assert_eq!(health.request_counts[0].method, "thread/start");
    assert_eq!(health.request_counts[0].outcome, "ok");
    assert_eq!(health.request_counts[0].count, 1);
    assert_eq!(health.last_request_method, Some("thread/start".to_string()));
    assert_eq!(health.last_request_outcome, Some("ok".to_string()));
    assert_eq!(health.last_request_at.is_some(), true);
}
