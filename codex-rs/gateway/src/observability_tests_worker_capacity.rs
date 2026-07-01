use super::*;
use pretty_assertions::assert_eq;

#[test]
fn records_v2_worker_reconnect_metrics_with_worker_and_outcome_tags() {
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

    observability.record_v2_worker_reconnect(7, "success");

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
        if metric.name() == V2_WORKER_RECONNECT_COUNT_METRIC {
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
                                ("outcome".to_string(), "success".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected worker reconnect count aggregation"),
                },
                _ => panic!("unexpected worker reconnect count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.worker_reconnect_event_counts,
        BTreeMap::from([("success".to_string(), 1)])
    );
    assert_eq!(
        health.last_worker_reconnect_event,
        Some("success".to_string())
    );
    assert_eq!(health.last_worker_reconnect_event_worker_id, Some(7));
    assert_eq!(health.last_worker_reconnect_event_at.is_some(), true);
}

#[test]
fn records_account_capacity_event_metrics_with_worker_and_event_tags() {
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

    observability.record_account_capacity_event(7, "exhausted");

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
        if metric.name() == ACCOUNT_CAPACITY_EVENT_COUNT_METRIC {
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
fn records_worker_pool_inventory_metrics_with_kind_tags() {
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

    observability.record_worker_pool_inventory(&GatewayWorkerPoolSnapshot {
        account_count: 2,
        leased_account_count: 1,
        policy_eligible_account_count: 1,
        policy_ineligible_account_count: 1,
        worker_slot_count: 3,
        bound_worker_slot_count: 2,
        accounts: vec![GatewayAccountPoolEntry {
            account_id: "acct-a".to_string(),
            lease_state: GatewayAccountLeaseState::Leased,
            leased_worker_id: Some(0),
            project_route_count: 0,
            account_capacity: GatewayAccountCapacityStatus::Available,
            account_capacity_reason: None,
            policy_eligible: true,
            policy_ineligibility_reason: None,
            cooldown_reason: None,
            last_error: None,
        }],
        worker_slots: vec![GatewayWorkerPoolSlot {
            worker_id: 0,
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            account_id: Some("acct-a".to_string()),
            account_login_state_path: None,
        }],
    });

    let resource_metrics = observability
        .metrics
        .as_ref()
        .expect("metrics client")
        .snapshot()
        .expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut inventory = BTreeMap::new();
    for metric in metrics {
        if metric.name() == WORKER_POOL_INVENTORY_METRIC {
            match metric.data() {
                AggregatedMetrics::I64(data) => match data {
                    MetricData::Gauge(gauge) => {
                        for point in gauge.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let kind = attributes.get("kind").expect("kind tag").clone();
                            inventory.insert(kind, point.value());
                        }
                    }
                    _ => panic!("unexpected worker-pool inventory aggregation"),
                },
                _ => panic!("unexpected worker-pool inventory type"),
            }
        }
    }

    assert_eq!(
        inventory,
        BTreeMap::from([
            ("accounts".to_string(), 2),
            ("leased_accounts".to_string(), 1),
            ("policy_eligible_accounts".to_string(), 1),
            ("policy_ineligible_accounts".to_string(), 1),
            ("worker_slots".to_string(), 3),
            ("bound_worker_slots".to_string(), 2),
        ])
    );
}
