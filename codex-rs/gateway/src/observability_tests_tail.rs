use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_server_request_lifecycle_metrics_with_event_tags() {
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

    observability.record_v2_server_request_lifecycle_event(
        "duplicate_resolved_replay",
        "serverRequest/resolved",
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
        if metric.name() == V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC {
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
                                ("event".to_string(), "duplicate_resolved_replay".to_string(),),
                                ("method".to_string(), "serverRequest/resolved".to_string(),),
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
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.server_request_lifecycle_event_counts,
        vec![GatewayV2ServerRequestLifecycleEventCounts {
            event: "duplicate_resolved_replay".to_string(),
            method: "serverRequest/resolved".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_server_request_lifecycle_event,
        Some("duplicate_resolved_replay".to_string())
    );
    assert_eq!(
        health.last_server_request_lifecycle_method,
        Some("serverRequest/resolved".to_string())
    );
    assert_eq!(health.last_server_request_lifecycle_at.is_some(), true);
}

#[test]
fn records_v2_protocol_violation_metrics_with_phase_and_reason_tags() {
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

    observability.record_v2_protocol_violation("post_initialize", "invalid_jsonrpc");

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
        if metric.name() == V2_PROTOCOL_VIOLATION_COUNT_METRIC {
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
                                ("phase".to_string(), "post_initialize".to_string()),
                                ("reason".to_string(), "invalid_jsonrpc".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected protocol violation count aggregation"),
                },
                _ => panic!("unexpected protocol violation count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.protocol_violation_counts,
        vec![GatewayV2ProtocolViolationCounts {
            phase: "post_initialize".to_string(),
            reason: "invalid_jsonrpc".to_string(),
            count: 1,
        }]
    );
    assert_eq!(health.protocol_violation_worker_counts, vec![]);
    assert_eq!(
        health.last_protocol_violation_phase,
        Some("post_initialize".to_string())
    );
    assert_eq!(
        health.last_protocol_violation_reason,
        Some("invalid_jsonrpc".to_string())
    );
    assert_eq!(health.last_protocol_violation_worker_id, None);
    assert_eq!(health.last_protocol_violation_at.is_some(), true);
}

#[test]
fn records_v2_worker_protocol_violation_in_health() {
    let observability = GatewayObservability::default();

    observability.record_v2_worker_protocol_violation(Some(3), "downstream", "invalid_jsonrpc");
    observability.record_v2_worker_protocol_violation(Some(3), "downstream", "invalid_jsonrpc");

    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.protocol_violation_worker_counts,
        vec![crate::api::GatewayV2ProtocolViolationWorkerCounts {
            worker_id: 3,
            violation_counts: vec![GatewayV2ProtocolViolationCounts {
                phase: "downstream".to_string(),
                reason: "invalid_jsonrpc".to_string(),
                count: 2,
            }],
        }]
    );
    assert_eq!(
        health.last_protocol_violation_phase,
        Some("downstream".to_string())
    );
    assert_eq!(
        health.last_protocol_violation_reason,
        Some("invalid_jsonrpc".to_string())
    );
    assert_eq!(health.last_protocol_violation_worker_id, Some(3));
    assert_eq!(health.last_protocol_violation_at.is_some(), true);
}

#[test]
fn records_v2_downstream_backpressure_metrics_with_worker_tag() {
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

    observability.record_v2_downstream_backpressure(Some(3));

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
        if metric.name() == V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC {
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
                            BTreeMap::from([("worker_id".to_string(), "3".to_string()),])
                        );
                    }
                    _ => panic!("unexpected downstream backpressure count aggregation"),
                },
                _ => panic!("unexpected downstream backpressure count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.downstream_backpressure_counts,
        vec![GatewayV2DownstreamBackpressureCounts {
            worker_id: Some(3),
            count: 1,
        }]
    );
    assert_eq!(health.last_downstream_backpressure_worker_id, Some(3));
    assert_eq!(health.last_downstream_backpressure_at.is_some(), true);
}

#[test]
fn records_v2_client_send_timeout_metrics() {
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

    observability.record_v2_client_send_timeout();

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
        if metric.name() == V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC {
            saw_count = true;
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
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(health.client_send_timeout_count, 1);
    assert_eq!(health.last_client_send_timeout_at.is_some(), true);
}
