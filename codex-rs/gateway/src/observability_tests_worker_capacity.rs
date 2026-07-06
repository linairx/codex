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
        available_account_count: 0,
        leased_account_count: 1,
        cooldown_account_count: 0,
        policy_eligible_account_count: 1,
        policy_ineligible_account_count: 1,
        worker_slot_count: 3,
        bound_worker_slot_count: 2,
        healthy_worker_slot_count: 0,
        unhealthy_worker_slot_count: 0,
        reconnecting_worker_slot_count: 0,
        account_login_state_path_count: 1,
        worker_account_login_state_path_count: 1,
        pool_account_login_state_path_count: 0,
        accounts: vec![GatewayAccountPoolEntry {
            account_id: "acct-a".to_string(),
            account_login_state_path: None,
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
            account_login_state_path: Some("/codex-home/acct-a".to_string()),
            account_capacity: None,
            account_capacity_reason: None,
            healthy: None,
            reconnecting: None,
            last_error: None,
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
            ("available_accounts".to_string(), 0),
            ("leased_accounts".to_string(), 1),
            ("cooldown_accounts".to_string(), 0),
            ("policy_eligible_accounts".to_string(), 1),
            ("policy_ineligible_accounts".to_string(), 1),
            ("worker_slots".to_string(), 3),
            ("bound_worker_slots".to_string(), 2),
            ("healthy_worker_slots".to_string(), 0),
            ("unhealthy_worker_slots".to_string(), 0),
            ("reconnecting_worker_slots".to_string(), 0),
            ("account_login_state_paths".to_string(), 1),
            ("worker_account_login_state_paths".to_string(), 1),
            ("pool_account_login_state_paths".to_string(), 0),
        ])
    );
}

#[test]
fn records_worker_pool_account_lease_metrics_with_account_and_worker_tags() {
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

    observability.record_worker_pool_account_leases(&GatewayWorkerPoolSnapshot {
        account_count: 2,
        available_account_count: 0,
        leased_account_count: 1,
        cooldown_account_count: 1,
        policy_eligible_account_count: 1,
        policy_ineligible_account_count: 1,
        worker_slot_count: 1,
        bound_worker_slot_count: 1,
        healthy_worker_slot_count: 0,
        unhealthy_worker_slot_count: 0,
        reconnecting_worker_slot_count: 0,
        account_login_state_path_count: 0,
        worker_account_login_state_path_count: 0,
        pool_account_login_state_path_count: 0,
        accounts: vec![
            GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: true,
                policy_ineligibility_reason: None,
                cooldown_reason: None,
                last_error: None,
            },
            GatewayAccountPoolEntry {
                account_id: "acct-b".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Cooldown,
                leased_worker_id: None,
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Exhausted,
                account_capacity_reason: Some("quota exhausted".to_string()),
                policy_eligible: false,
                policy_ineligibility_reason: Some("quota exhausted".to_string()),
                cooldown_reason: Some("quota exhausted".to_string()),
                last_error: None,
            },
        ],
        worker_slots: vec![GatewayWorkerPoolSlot {
            worker_id: 0,
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            account_id: Some("acct-a".to_string()),
            account_login_state_path: None,
            account_capacity: None,
            account_capacity_reason: None,
            healthy: None,
            reconnecting: None,
            last_error: None,
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

    let mut lease_events = BTreeMap::new();
    for metric in metrics {
        if metric.name() == ACCOUNT_LEASE_EVENT_COUNT_METRIC {
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
                            let key = (
                                attributes.get("event").expect("event tag").clone(),
                                attributes.get("account_id").expect("account tag").clone(),
                                attributes.get("worker_id").expect("worker tag").clone(),
                            );
                            lease_events.insert(key, point.value());
                        }
                    }
                    _ => panic!("unexpected account lease event count aggregation"),
                },
                _ => panic!("unexpected account lease event count type"),
            }
        }
    }

    assert_eq!(
        lease_events,
        BTreeMap::from([
            (
                ("leased".to_string(), "acct-a".to_string(), "0".to_string(),),
                1,
            ),
            (
                (
                    "cooldown".to_string(),
                    "acct-b".to_string(),
                    "none".to_string(),
                ),
                1,
            ),
        ])
    );
}
