use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_project_worker_route_selection_metrics_with_worker_tenant_project_and_account_tags() {
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

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        Some("acct-a"),
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
        if metric.name() == super::PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC {
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
                                ("worker_id".to_string(), "7".to_string()),
                                ("tenant_id".to_string(), "tenant-a".to_string()),
                                ("project_id".to_string(), "project-a".to_string()),
                                ("account_id".to_string(), "acct-a".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected project worker route metric aggregation"),
                },
                _ => panic!("unexpected project worker route metric type"),
            }
        }
    }

    assert!(saw_count);

    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.project_worker_route_selection_count, 1);
    assert_eq!(
        health.project_worker_route_selection_worker_counts,
        vec![GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
            worker_id: 7,
            project_worker_route_selection_count: 1,
        }]
    );
    assert_eq!(health.last_project_worker_route_selected_worker_id, Some(7));
    assert_eq!(
        health
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-a")
    );
    assert_eq!(
        health
            .last_project_worker_route_selected_account_id
            .as_deref(),
        Some("acct-a")
    );
    assert!(health.last_project_worker_route_selected_at.is_some());
}

#[test]
fn records_project_worker_route_selection_metrics_without_account_id() {
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

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        None,
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
        if metric.name() == super::PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC {
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
                                ("worker_id".to_string(), "7".to_string()),
                                ("tenant_id".to_string(), "tenant-a".to_string()),
                                ("project_id".to_string(), "project-a".to_string()),
                                ("account_id".to_string(), "none".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected project worker route metric aggregation"),
                },
                _ => panic!("unexpected project worker route metric type"),
            }
        }
    }

    assert!(saw_count);

    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.project_worker_route_selection_count, 1);
    assert_eq!(health.last_project_worker_route_selected_worker_id, Some(7));
    assert_eq!(
        health
            .last_project_worker_route_selected_account_id
            .as_deref(),
        None
    );
    assert!(health.last_project_worker_route_selected_at.is_some());
}

#[test]
fn emits_project_worker_route_selection_audit_log() {
    let observability = GatewayObservability::new(None, true);

    let logs = capture_logs(|| {
        observability.record_project_worker_route_selected(
            7,
            "tenant-a",
            "project-a",
            "thread-a",
            Some("acct-a"),
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(
        logs.contains("gateway project worker route selected"),
        "{logs}"
    );
    assert!(logs.contains("worker_id=7"), "{logs}");
    assert!(logs.contains("tenant_id=\"tenant-a\""), "{logs}");
    assert!(logs.contains("project_id=\"project-a\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-a\""), "{logs}");
    assert!(logs.contains("account_id=\"acct-a\""), "{logs}");
}

#[test]
fn emits_project_worker_route_selection_audit_log_without_account_id() {
    let observability = GatewayObservability::new(None, true);

    let logs = capture_logs(|| {
        observability.record_project_worker_route_selected(
            7,
            "tenant-a",
            "project-a",
            "thread-a",
            None,
        );
    });

    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(
        logs.contains("gateway project worker route selected"),
        "{logs}"
    );
    assert!(logs.contains("worker_id=7"), "{logs}");
    assert!(logs.contains("tenant_id=\"tenant-a\""), "{logs}");
    assert!(logs.contains("project_id=\"project-a\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-a\""), "{logs}");
    assert!(logs.contains("account_id=\"<none>\""), "{logs}");
}
