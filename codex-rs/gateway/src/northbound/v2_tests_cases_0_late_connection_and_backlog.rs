use super::*;
use crate::northbound::v2_connection::GatewayV2ReconnectState;
use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::northbound::v2_server_request_cleanup::record_worker_cleanup_resolution_send_failure;
use crate::northbound::v2_server_request_cleanup::reject_pending_server_requests;
use crate::northbound::v2_server_request_cleanup::should_reject_pending_server_requests_after_connection_error;
use crate::northbound::v2_server_requests::collect_server_request_cleanup_for_worker;
use crate::northbound::v2_server_requests::record_client_server_request_cleanup_metrics;
use crate::northbound::v2_server_requests::record_worker_server_request_cleanup_metrics;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::observe_v2_connection;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;

#[path = "v2_tests_cases_0_late_connection_and_backlog_cleanup.rs"]
mod v2_tests_cases_0_late_connection_and_backlog_cleanup;

#[test]
fn aggregated_page_bounds_returns_requested_window_when_offset_is_in_range() {
    pretty_assertions::assert_eq!(
        aggregated_page_bounds(5, 1, 2, "apps-offset:"),
        (1, 3, Some("apps-offset:3".to_string()))
    );
}

#[test]
fn aggregated_page_bounds_returns_empty_page_when_offset_is_past_end() {
    pretty_assertions::assert_eq!(
        aggregated_page_bounds(2, 5, 3, "apps-offset:"),
        (2, 2, None)
    );
}

#[test]
fn connection_errors_that_end_the_northbound_socket_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidData,
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::BrokenPipe,
        std::io::ErrorKind::ConnectionAborted,
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::UnexpectedEof,
        std::io::ErrorKind::WriteZero,
        std::io::ErrorKind::NotConnected,
    ] {
        let err = std::io::Error::new(kind, "test");
        pretty_assertions::assert_eq!(
            should_reject_pending_server_requests_after_connection_error(&err),
            true
        );
    }
}

#[test]
fn non_terminal_connection_errors_do_not_reject_pending_server_requests() {
    for kind in [
        std::io::ErrorKind::InvalidInput,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::Other,
    ] {
        let err = std::io::Error::new(kind, "test");
        pretty_assertions::assert_eq!(
            should_reject_pending_server_requests_after_connection_error(&err),
            false
        );
    }
}

#[test]
fn classify_v2_connection_error_maps_timeout_and_disconnect_kinds() {
    pretty_assertions::assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        )),
        "client_send_timed_out"
    );
    pretty_assertions::assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "closed",
        )),
        "client_disconnected"
    );
    pretty_assertions::assert_eq!(
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid",
        )),
        "protocol_violation"
    );
    pretty_assertions::assert_eq!(
        classify_v2_connection_error(&std::io::Error::other("other")),
        "connection_error"
    );
}

#[test]
fn observe_v2_connection_records_client_send_timeout_outcome() {
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let context = GatewayRequestContext::default();
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();
    observability
        .v2_connection_health()
        .update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 6,
                pending_client_request_worker_counts: Vec::new(),
                pending_client_request_method_counts: Vec::new(),
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: Vec::new(),
                server_request_backlog_method_counts: Vec::new(),
            },
        );

    observe_v2_connection(
        &observability,
        connection_id,
        &context,
        classify_v2_connection_error(&std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "gateway websocket send timed out",
        )),
        None,
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 4,
            pending_client_request_worker_counts: Vec::new(),
            pending_client_request_method_counts: vec![
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 3,
                },
                crate::api::GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 1,
                },
            ],
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: Vec::new(),
            server_request_backlog_method_counts: Vec::new(),
        },
        Duration::from_millis(9),
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    pretty_assertions::assert_eq!(health_snapshot.active_connection_count, 0);
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_outcome,
        Some("client_send_timed_out".to_string())
    );
    pretty_assertions::assert_eq!(health_snapshot.last_connection_detail, None);
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_pending_client_request_count,
        4
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_max_pending_client_request_count,
        6
    );
    pretty_assertions::assert_eq!(
        health_snapshot
            .last_connection_pending_client_request_started_at
            .is_some(),
        true
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_pending_client_request_method_counts,
        vec![
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 3,
            },
            crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "thread/read".to_string(),
                pending_client_request_count: 1,
            },
        ]
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_pending_server_request_count,
        2
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_answered_but_unresolved_server_request_count,
        1
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_server_request_backlog_count,
        3
    );
    pretty_assertions::assert_eq!(
        health_snapshot.last_connection_max_server_request_backlog_count,
        5
    );
    pretty_assertions::assert_eq!(health_snapshot.last_connection_completed_at.is_some(), true);
    pretty_assertions::assert_eq!(health_snapshot.last_connection_duration_ms, Some(9));

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_method_points = Vec::new();
    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_connections" => {
                saw_count = true;
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            pretty_assertions::assert_eq!(point.value(), 1);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection count aggregation"),
                    },
                    _ => panic!("unexpected v2 connection count type"),
                }
            }
            "gateway_v2_connection_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 9.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection duration aggregation"),
                    },
                    _ => panic!("unexpected v2 connection duration type"),
                }
            }
            "gateway_v2_connection_pending_client_requests" => {
                saw_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 4.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending client request aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending client request type"),
                }
            }
            "gateway_v2_connection_max_pending_client_requests" => {
                saw_max_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 6.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max pending client request aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max pending client request type"),
                }
            }
            "gateway_v2_connection_pending_client_requests_by_method" => match metric.data() {
                AggregatedMetrics::F64(data) => match data {
                    MetricData::Histogram(histogram) => {
                        pending_client_request_method_points.extend(histogram.data_points().map(
                            |point| {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                (attributes, point.count(), point.sum())
                            },
                        ));
                    }
                    _ => panic!(
                        "unexpected v2 connection pending client request by method aggregation"
                    ),
                },
                _ => panic!("unexpected v2 connection pending client request by method type"),
            },
            "gateway_v2_connection_pending_server_requests" => {
                saw_pending_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 2.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection pending aggregation"),
                    },
                    _ => panic!("unexpected v2 connection pending type"),
                }
            }
            "gateway_v2_connection_answered_but_unresolved_server_requests" => {
                saw_answered_but_unresolved_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 1.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection answered-but-unresolved aggregation"),
                    },
                    _ => panic!("unexpected v2 connection answered-but-unresolved type"),
                }
            }
            "gateway_v2_connection_server_request_backlog" => {
                saw_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 3.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected v2 connection server-request backlog aggregation"),
                    },
                    _ => panic!("unexpected v2 connection server-request backlog type"),
                }
            }
            "gateway_v2_connection_max_server_request_backlog" => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            pretty_assertions::assert_eq!(point.count(), 1);
                            pretty_assertions::assert_eq!(point.sum(), 5.0);
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            pretty_assertions::assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "outcome".to_string(),
                                    "client_send_timed_out".to_string(),
                                )])
                            );
                        }
                        _ => panic!(
                            "unexpected v2 connection max server-request backlog aggregation"
                        ),
                    },
                    _ => panic!("unexpected v2 connection max server-request backlog type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    pretty_assertions::assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                3.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string()),
                    ("outcome".to_string(), "client_send_timed_out".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests);
    assert!(saw_answered_but_unresolved_server_requests);
    assert!(saw_server_request_backlog);
    assert!(saw_max_server_request_backlog);
}
