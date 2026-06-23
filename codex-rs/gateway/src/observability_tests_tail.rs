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

#[test]
fn emits_info_log_for_normal_v2_connection_outcome() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_log(
            "client_disconnected",
            Duration::from_millis(7),
            &context,
            None,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 4,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(2),
                        pending_client_request_count: 4,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 4,
                    },
                ],
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                ],
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 9,
                max_server_request_backlog_count: 8,
            },
        );
    });

    assert!(logs.contains("INFO"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("client_disconnected"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("project-a"));
    assert!(logs.contains("7"));
    assert!(logs.contains("pending_client_request_count=4"));
    assert!(logs.contains("max_pending_client_request_count=9"));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("method: \"command/exec\""));
    assert!(logs.contains("pending_server_request_count=2"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(logs.contains("server_request_backlog_count=3"));
    assert!(logs.contains("max_server_request_backlog_count=8"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(2)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"item/tool/requestUserInput\""));
}

#[test]
fn emits_warn_log_for_non_normal_v2_connection_outcome() {
    let observability = GatewayObservability::new(None, false);
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: None,
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_log(
            "downstream_backpressure",
            Duration::from_millis(11),
            &context,
            Some("downstream app-server event stream lagged"),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 5,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 6,
                max_server_request_backlog_count: 7,
            },
        );
    });

    assert!(logs.contains("WARN"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("downstream_backpressure"));
    assert!(logs.contains("downstream app-server event stream lagged"));
    assert!(logs.contains("tenant-a"));
    assert!(logs.contains("11"));
    assert!(logs.contains("pending_client_request_count=5"));
    assert!(logs.contains("max_pending_client_request_count=6"));
    assert!(logs.contains("pending_server_request_count=3"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=2"));
    assert!(logs.contains("server_request_backlog_count=5"));
    assert!(logs.contains("max_server_request_backlog_count=7"));
}

#[test]
fn emits_v2_connection_audit_log_with_server_request_counts() {
    let observability = GatewayObservability::new(None, true);
    let context = GatewayRequestContext {
        tenant_id: "tenant-audit".to_string(),
        project_id: Some("project-audit".to_string()),
    };
    let logs = capture_logs(|| {
        observability.emit_v2_connection_audit_log(
            "client_send_timed_out",
            Duration::from_millis(17),
            &context,
            Some("gateway websocket send timed out"),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: vec![
                    GatewayV2PendingClientRequestWorkerCounts {
                        worker_id: Some(7),
                        pending_client_request_count: 6,
                    },
                ],
                pending_client_request_method_counts: vec![
                    GatewayV2PendingClientRequestMethodCounts {
                        method: "command/exec".to_string(),
                        pending_client_request_count: 6,
                    },
                ],
                pending_server_request_count: 4,
                answered_but_unresolved_server_request_count: 3,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(7),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 7,
                    },
                ],
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "serverRequest/elicitation".to_string(),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 3,
                        server_request_backlog_count: 7,
                    },
                ],
            },
            &GatewayV2ConnectionCompletionCounts {
                max_pending_client_request_count: 10,
                max_server_request_backlog_count: 11,
            },
        );
    });

    assert!(logs.contains("codex_gateway.audit"));
    assert!(logs.contains("gateway v2 connection completed"));
    assert!(logs.contains("client_send_timed_out"));
    assert!(logs.contains("tenant-audit"));
    assert!(logs.contains("project-audit"));
    assert!(logs.contains("17"));
    assert!(logs.contains("gateway websocket send timed out"));
    assert!(logs.contains("pending_client_request_count=6"));
    assert!(logs.contains("max_pending_client_request_count=10"));
    assert!(logs.contains("pending_client_request_worker_counts=["));
    assert!(logs.contains("pending_client_request_method_counts=["));
    assert!(logs.contains("method: \"command/exec\""));
    assert!(logs.contains("pending_server_request_count=4"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=3"));
    assert!(logs.contains("server_request_backlog_count=7"));
    assert!(logs.contains("max_server_request_backlog_count=11"));
    assert!(logs.contains("server_request_backlog_worker_counts=["));
    assert!(logs.contains("worker_id: Some(7)"));
    assert!(logs.contains("server_request_backlog_method_counts=["));
    assert!(logs.contains("method: \"serverRequest/elicitation\""));
}
