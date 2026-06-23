use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_remote_account_label_event_metrics_with_worker_and_event_tags() {
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

    observability.record_remote_account_label_event(3, "unlabeled");

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
        if metric.name() == REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC {
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
                                ("worker_id".to_string(), "3".to_string()),
                                ("event".to_string(), "unlabeled".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected remote account label event count aggregation"),
                },
                _ => panic!("unexpected remote account label event count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_server_request_lifecycle_metrics_with_event_tags() {
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

    observability.record_server_request_lifecycle_event(
        "client_server_request_delivery_failed",
        "toolRequestUserInput",
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
    for metric in metrics {
        if metric.name() == SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC {
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
                                    "event".to_string(),
                                    "client_server_request_delivery_failed".to_string(),
                                ),
                                ("method".to_string(), "toolRequestUserInput".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_server_request_answer_delivery_failure_metrics_with_response_kind_tags() {
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

    observability.record_server_request_answer_delivery_failure("toolRequestUserInput");

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
        if metric.name() == SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC {
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
                                "response_kind".to_string(),
                                "toolRequestUserInput".to_string(),
                            )])
                        );
                    }
                    _ => panic!("unexpected server-request delivery failure aggregation"),
                },
                _ => panic!("unexpected server-request delivery failure type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_account_capacity_event_metrics_with_worker_and_event_tags() {
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

    observability.record_v2_account_capacity_event(7, "exhausted", None, None);
    observability.record_v2_account_capacity_event(7, "exhausted", None, None);
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .account_capacity_event_counts,
        BTreeMap::from([("exhausted".to_string(), 2)])
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
    for metric in metrics {
        if metric.name() == V2_ACCOUNT_CAPACITY_EVENT_COUNT_METRIC {
            saw_count = true;
            match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 2);
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
                                ("worker_id".to_string(), "7".to_string()),
                                ("event".to_string(), "exhausted".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected account capacity event count aggregation"),
                },
                _ => panic!("unexpected account capacity event count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_fail_closed_request_metrics_with_method_and_backoff_tags() {
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

    observability.record_v2_fail_closed_request("config/read", true);

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
        if metric.name() == V2_FAIL_CLOSED_REQUEST_COUNT_METRIC {
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
                                ("method".to_string(), "config/read".to_string()),
                                ("reconnect_backoff_active".to_string(), "true".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected fail-closed request count aggregation"),
                },
                _ => panic!("unexpected fail-closed request count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.fail_closed_request_counts,
        vec![GatewayV2FailClosedRequestCounts {
            method: "config/read".to_string(),
            reconnect_backoff_active: true,
            count: 1,
        }]
    );
    assert_eq!(
        health.last_fail_closed_request_method,
        Some("config/read".to_string())
    );
    assert_eq!(
        health.last_fail_closed_request_reconnect_backoff_active,
        Some(true)
    );
    assert_eq!(health.last_fail_closed_request_at.is_some(), true);
}

#[test]
fn records_v2_upstream_request_failure_metrics_with_method_and_backoff_tags() {
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

    observability.record_v2_upstream_request_failure("config/read", true);

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
        if metric.name() == V2_UPSTREAM_REQUEST_FAILURE_COUNT_METRIC {
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
                                ("method".to_string(), "config/read".to_string()),
                                ("reconnect_backoff_active".to_string(), "true".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected upstream request failure count aggregation"),
                },
                _ => panic!("unexpected upstream request failure count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.upstream_request_failure_counts,
        vec![GatewayV2UpstreamRequestFailureCounts {
            method: "config/read".to_string(),
            reconnect_backoff_active: true,
            count: 1,
        }]
    );
    assert_eq!(
        health.last_upstream_request_failure_method,
        Some("config/read".to_string())
    );
    assert_eq!(
        health.last_upstream_request_failure_reconnect_backoff_active,
        Some(true)
    );
    assert_eq!(health.last_upstream_request_failure_at.is_some(), true);
}
