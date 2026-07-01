use super::*;
use pretty_assertions::assert_eq;

pub(super) fn assert_server_request_lifecycle_and_delivery_failure_metrics(
    metrics: &codex_otel::MetricsClient,
    response_kind: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let expected_lifecycle_points = vec![
        (
            BTreeMap::from([
                (
                    "event".to_string(),
                    "client_server_request_answered".to_string(),
                ),
                ("method".to_string(), response_kind.to_string()),
            ]),
            1_u64,
        ),
        (
            BTreeMap::from([
                (
                    "event".to_string(),
                    "client_server_request_delivery_failed".to_string(),
                ),
                ("method".to_string(), response_kind.to_string()),
            ]),
            1_u64,
        ),
    ];
    let mut lifecycle_points = Vec::new();
    let mut saw_delivery_failure_count = false;

    for metric in metrics {
        match metric.name() {
            "gateway_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        lifecycle_points.extend(sum.data_points().map(|point| {
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
                    _ => panic!("unexpected server-request lifecycle aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle metric type"),
            },
            "gateway_server_request_answer_delivery_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
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
                                response_kind.to_string()
                            )])
                        );
                        assert_eq!(point.value(), 1);
                        saw_delivery_failure_count = true;
                    }
                    _ => panic!("unexpected server-request delivery failure aggregation"),
                },
                _ => panic!("unexpected server-request delivery failure metric type"),
            },
            _ => {}
        }
    }

    lifecycle_points.sort();
    assert_eq!(lifecycle_points, expected_lifecycle_points);
    assert!(saw_delivery_failure_count);
}

pub(super) fn assert_server_request_lifecycle_metric(
    metrics: &codex_otel::MetricsClient,
    event: &str,
    method: &str,
) {
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let expected_attributes = BTreeMap::from([
        ("event".to_string(), event.to_string()),
        ("method".to_string(), method.to_string()),
    ]);
    let mut saw_count = false;

    for metric in metrics {
        if metric.name() == "gateway_server_request_lifecycle_events" {
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
                            if attributes == expected_attributes {
                                assert_eq!(point.value(), 1);
                                saw_count = true;
                            }
                        }
                    }
                    _ => panic!("unexpected server-request lifecycle aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle metric type"),
            }
        }
    }

    assert!(
        saw_count,
        "missing gateway_server_request_lifecycle_events metric point: {expected_attributes:?}"
    );
}

#[test]
fn reports_degraded_remote_health_when_some_workers_are_unhealthy() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        ("ws://127.0.0.1:8081".to_string(), None),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        1,
    );
    scope_registry.register_pending_server_request_with_worker(
        RequestId::String("req-1".to_string()),
        GatewayServerRequestKind::ToolRequestUserInput,
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        "thread-1".to_string(),
        Some(1),
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health,
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    assert_eq!(runtime.health().status, GatewayHealthStatus::Degraded);
    let health = runtime.health();
    assert_eq!(health.runtime_mode, "remote".to_string());
    assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(
        health.v2_transport,
        GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        }
    );
    assert_eq!(health.v2_connections, GatewayV2ConnectionHealth::default());
    assert_eq!(health.pending_server_request_count, 1);
    assert_eq!(
        health.pending_server_request_kind_counts,
        [("toolRequestUserInput".to_string(), 1)].into()
    );
    assert_eq!(
        health.pending_server_request_route_counts,
        vec![crate::api::GatewayPendingServerRequestRouteCounts {
            worker_id: Some(1),
            count: 1,
            kind_counts: [("toolRequestUserInput".to_string(), 1)].into(),
        }]
    );
    assert!(health.pending_server_request_oldest_at.is_some());
    assert_eq!(health.remote_account_labels_complete, Some(false));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(1));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(vec![0]));
    assert_eq!(
        health.remote_unlabeled_account_workers,
        Some(vec![crate::api::GatewayRemoteUnlabeledAccountWorker {
            worker_id: 0,
            websocket_url: "ws://127.0.0.1:8081".to_string(),
        }])
    );
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(
        remote_workers[0],
        crate::api::GatewayRemoteWorkerHealth {
            worker_id: 0,
            websocket_url: "ws://127.0.0.1:8081".to_string(),
            account_id: None,
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            account_capacity_reason: None,
            account_capacity_last_changed_at: None,
            healthy: true,
            reconnecting: false,
            reconnect_attempt_count: 0,
            last_error: None,
            last_state_change_at: None,
            last_error_at: None,
            next_reconnect_at: None,
            reconnect_backoff_remaining_seconds: None,
        }
    );
    assert_eq!(remote_workers[1].worker_id, 1);
    assert_eq!(remote_workers[1].websocket_url, "ws://127.0.0.1:8082");
    assert_eq!(remote_workers[1].account_id, Some("acct-b".to_string()));
    assert_eq!(remote_workers[1].healthy, false);
    assert_eq!(remote_workers[1].reconnecting, false);
    assert_eq!(remote_workers[1].reconnect_attempt_count, 0);
    assert_eq!(
        remote_workers[1].last_error.as_deref(),
        Some("socket closed")
    );
    assert_eq!(remote_workers[1].last_state_change_at.is_some(), true);
    assert_eq!(remote_workers[1].last_error_at.is_some(), true);
    assert_eq!(remote_workers[1].next_reconnect_at, None);
    assert_eq!(
        health.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: false,
            account_routing_eligible: false,
        }])
    );
}

#[test]
fn reports_complete_remote_account_labels_for_labeled_multi_worker_health() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        1,
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health,
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let health = runtime.health();

    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteMultiWorker
    );
    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(
        remote_workers
            .iter()
            .map(|worker| worker.account_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("acct-a"), Some("acct-b")]
    );
    assert_eq!(
        health.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: true,
            account_routing_eligible: true,
        }])
    );
}

#[test]
fn reports_project_worker_routes_for_multiple_labeled_projects() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        0,
    );
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        },
        1,
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health,
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let health = runtime.health();

    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    assert_eq!(
        health
            .remote_workers
            .expect("remote workers")
            .iter()
            .map(|worker| worker.account_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("acct-a"), Some("acct-b")]
    );
    assert_eq!(
        health.project_worker_routes,
        Some(vec![
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
        ])
    );
}

#[test]
fn reports_mixed_project_worker_route_eligibility_after_worker_health_changes() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        0,
    );
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-b".to_string()),
        },
        1,
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health: worker_health.clone(),
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
    worker_health.mark_account_exhausted_for_worker(1, "quota exceeded".to_string());

    let health = runtime.health();

    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    assert_eq!(
        health.project_worker_routes,
        Some(vec![
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
                worker_healthy: false,
                account_routing_eligible: false,
            },
        ])
    );
}

#[test]
fn remote_health_treats_blank_account_labels_as_unlabeled_for_project_routes() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        ("ws://127.0.0.1:8082".to_string(), Some("   ".to_string())),
    ]));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        /*worker_id*/ 1,
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health,
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    let health = runtime.health();

    assert_eq!(health.remote_account_labels_complete, Some(false));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(1));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(vec![1]));
    assert_eq!(
        health.remote_unlabeled_account_workers,
        Some(vec![crate::api::GatewayRemoteUnlabeledAccountWorker {
            worker_id: 1,
            websocket_url: "ws://127.0.0.1:8082".to_string(),
        }])
    );
    let remote_workers = health.remote_workers.expect("remote workers");
    assert_eq!(
        remote_workers
            .iter()
            .map(|worker| worker.account_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("acct-a"), None]
    );
    assert_eq!(
        health.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: None,
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: true,
            account_routing_eligible: false,
        }])
    );
}

#[test]
fn project_worker_routes_track_worker_health_and_capacity_changes() {
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:8082".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_project_worker(
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
        1,
    );
    let runtime = RemoteWorkerGatewayRuntime {
        workers: Vec::new(),
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        next_worker: AtomicUsize::new(0),
        next_request_id: Arc::new(AtomicI64::new(1)),
        events: broadcast::channel(4).0,
        scope_registry,
        worker_health: worker_health.clone(),
        worker_pool: empty_worker_pool(),
        v2_transport: GatewayV2TransportConfig {
            initialize_timeout_seconds: 30,
            client_send_timeout_seconds: 10,
            reconnect_retry_backoff_seconds: 1,
            max_pending_server_requests: 64,
            max_pending_client_requests: 64,
        },
        v2_connection_health: empty_v2_connection_health(),
        observability: GatewayObservability::default(),
    };

    runtime
        .v2_connection_health
        .record_project_worker_route_selected(
            1,
            "tenant-a",
            "project-a",
            "thread-a",
            Some("acct-b"),
        );

    let healthy_snapshot = runtime.health();
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .project_worker_route_selection_count,
        1
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 1,
                project_worker_route_selection_count: 1,
            }
        ]
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_worker_id,
        Some(1)
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-a")
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-a")
    );
    assert_eq!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_account_id
            .as_deref(),
        Some("acct-b")
    );
    assert!(
        healthy_snapshot
            .v2_connections
            .last_project_worker_route_selected_at
            .is_some()
    );
    assert_eq!(
        healthy_snapshot.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: true,
            account_routing_eligible: true,
        }])
    );

    worker_health.mark_account_exhausted_for_worker(1, "quota exceeded".to_string());

    let exhausted_snapshot = runtime.health();
    assert_eq!(
        exhausted_snapshot
            .v2_connections
            .project_worker_route_selection_count,
        1
    );
    assert_eq!(
        exhausted_snapshot.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
            worker_healthy: true,
            account_routing_eligible: false,
        }])
    );

    worker_health.mark_unhealthy(1, Some("socket closed".to_string()));

    let degraded_snapshot = runtime.health();
    assert_eq!(
        degraded_snapshot.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
            worker_healthy: false,
            account_routing_eligible: false,
        }])
    );

    worker_health.mark_healthy(1);
    worker_health.mark_account_available_for_worker(1);

    let recovered_snapshot = runtime.health();
    assert_eq!(
        recovered_snapshot
            .v2_connections
            .project_worker_route_selection_count,
        1
    );
    assert_eq!(
        recovered_snapshot.project_worker_routes,
        Some(vec![crate::api::GatewayProjectWorkerRoute {
            tenant_id: "tenant-a".to_string(),
            project_id: "project-a".to_string(),
            worker_id: 1,
            account_id: Some("acct-b".to_string()),
            account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
            worker_healthy: true,
            account_routing_eligible: true,
        }])
    );
}
