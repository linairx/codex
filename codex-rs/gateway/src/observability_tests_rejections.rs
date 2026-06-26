use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_server_request_rejection_metrics_with_reason_tags() {
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

    observability.record_v2_server_request_rejection("item/tool/requestUserInput", "pending_limit");
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.server_request_rejection_counts.len(), 1);
    assert_eq!(
        health.last_server_request_rejection_method,
        Some("item/tool/requestUserInput".to_string())
    );
    assert_eq!(
        health.last_server_request_rejection_reason,
        Some("pending_limit".to_string())
    );
    assert_eq!(health.last_server_request_rejection_at.is_some(), true);

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
        if metric.name() == V2_SERVER_REQUEST_REJECTION_COUNT_METRIC {
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
                                (
                                    "method".to_string(),
                                    "item/tool/requestUserInput".to_string(),
                                ),
                                ("reason".to_string(), "pending_limit".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected server-request rejection count aggregation"),
                },
                _ => panic!("unexpected server-request rejection count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_client_request_rejection_metrics_with_reason_tags() {
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

    observability.record_v2_client_request_rejection("command/exec", "pending_limit");
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.client_request_rejection_counts.len(), 1);
    assert_eq!(
        health.last_client_request_rejection_method,
        Some("command/exec".to_string())
    );
    assert_eq!(
        health.last_client_request_rejection_reason,
        Some("pending_limit".to_string())
    );
    assert_eq!(health.last_client_request_rejection_at.is_some(), true);

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
        if metric.name() == V2_CLIENT_REQUEST_REJECTION_COUNT_METRIC {
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
    }

    assert!(saw_count);
}
