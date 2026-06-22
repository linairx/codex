use super::ACCOUNT_CAPACITY_EVENT_COUNT_METRIC;
use super::GatewayObservability;
use super::REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC;
use super::REQUEST_COUNT_METRIC;
use super::REQUEST_DURATION_METRIC;
use super::SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC;
use super::SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC;
use super::V2_ACCOUNT_CAPACITY_EVENT_COUNT_METRIC;
use super::V2_CLIENT_REQUEST_REJECTION_COUNT_METRIC;
use super::V2_CLIENT_RESPONSE_SEND_FAILURE_COUNT_METRIC;
use super::V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC;
use super::V2_CLOSE_FRAME_SEND_FAILURE_COUNT_METRIC;
use super::V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC;
use super::V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC;
use super::V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC;
use super::V2_CONNECTION_COUNT_METRIC;
use super::V2_CONNECTION_DURATION_METRIC;
use super::V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC;
use super::V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC;
use super::V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC;
use super::V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC;
use super::V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC;
use super::V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC;
use super::V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC;
use super::V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC;
use super::V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC;
use super::V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC;
use super::V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC;
use super::V2_DEGRADED_THREAD_DISCOVERY_COUNT_METRIC;
use super::V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC;
use super::V2_DOWNSTREAM_SHUTDOWN_FAILURE_COUNT_METRIC;
use super::V2_FAIL_CLOSED_REQUEST_COUNT_METRIC;
use super::V2_FORWARDED_NOTIFICATION_COUNT_METRIC;
use super::V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC;
use super::V2_PROTOCOL_VIOLATION_COUNT_METRIC;
use super::V2_REQUEST_COUNT_METRIC;
use super::V2_REQUEST_DURATION_METRIC;
use super::V2_SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC;
use super::V2_SERVER_REQUEST_FORWARD_SEND_FAILURE_COUNT_METRIC;
use super::V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC;
use super::V2_SERVER_REQUEST_REJECTION_COUNT_METRIC;
use super::V2_SERVER_REQUEST_REJECTION_DELIVERY_FAILURE_COUNT_METRIC;
use super::V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC;
use super::V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC;
use super::V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC;
use super::V2_UPSTREAM_REQUEST_FAILURE_COUNT_METRIC;
use super::V2_WORKER_RECONNECT_COUNT_METRIC;
use crate::api::GatewayV2DegradedThreadDiscoveryCounts;
use crate::api::GatewayV2DownstreamBackpressureCounts;
use crate::api::GatewayV2FailClosedRequestCounts;
use crate::api::GatewayV2ForwardedNotificationCounts;
use crate::api::GatewayV2NotificationSendFailureCounts;
use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts;
use crate::api::GatewayV2ProtocolViolationCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use crate::api::GatewayV2ServerRequestLifecycleEventCounts;
use crate::api::GatewayV2ThreadListDeduplicationCounts;
use crate::api::GatewayV2ThreadRouteRecoveryCounts;
use crate::api::GatewayV2UpstreamRequestFailureCounts;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionCompletionCounts;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn records_request_metrics_with_route_and_status_tags() {
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

#[test]
fn records_v2_request_metrics_with_method_and_outcome_tags() {
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

#[test]
fn records_v2_connection_metrics_with_outcome_tags() {
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

    observability.record_v2_connection(
        "downstream_session_ended",
        Duration::from_millis(14),
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: 3,
            pending_client_request_worker_counts: vec![
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(2),
                    pending_client_request_count: 2,
                },
                GatewayV2PendingClientRequestWorkerCounts {
                    worker_id: Some(7),
                    pending_client_request_count: 1,
                },
            ],
            pending_client_request_method_counts: vec![
                GatewayV2PendingClientRequestMethodCounts {
                    method: "command/exec".to_string(),
                    pending_client_request_count: 2,
                },
                GatewayV2PendingClientRequestMethodCounts {
                    method: "thread/read".to_string(),
                    pending_client_request_count: 1,
                },
            ],
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 1,
            server_request_backlog_worker_counts: vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(3),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 2,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(8),
                    pending_server_request_count: 0,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 1,
                },
            ],
            server_request_backlog_method_counts: vec![GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 3,
            }],
        },
        5,
        7,
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
    let mut saw_duration = false;
    let mut saw_pending_client_requests = false;
    let mut saw_max_pending_client_requests = false;
    let mut pending_client_request_worker_points = Vec::new();
    let mut pending_client_request_method_points = Vec::new();
    let mut saw_pending_server_requests = false;
    let mut saw_answered_but_unresolved_server_requests = false;
    let mut pending_server_request_worker_points = Vec::new();
    let mut answered_but_unresolved_server_request_worker_points = Vec::new();
    let mut server_request_backlog_worker_points = Vec::new();
    let mut saw_pending_server_requests_by_method = false;
    let mut saw_answered_but_unresolved_server_requests_by_method = false;
    let mut saw_server_request_backlog = false;
    let mut saw_max_server_request_backlog = false;
    let mut saw_server_request_backlog_by_method = false;
    for metric in metrics {
        match metric.name() {
            name if name == V2_CONNECTION_COUNT_METRIC => {
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected connection count aggregation"),
                    },
                    _ => panic!("unexpected connection count type"),
                }
            }
            name if name == V2_CONNECTION_DURATION_METRIC => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 14.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected connection duration aggregation"),
                    },
                    _ => panic!("unexpected connection duration type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC => {
                saw_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected pending client request aggregation"),
                    },
                    _ => panic!("unexpected pending client request type"),
                }
            }
            name if name == V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC => {
                saw_max_pending_client_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 5.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected max pending client request aggregation"),
                    },
                    _ => panic!("unexpected max pending client request type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_method_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending client request by method aggregation")
                        }
                    },
                    _ => panic!("unexpected pending client request by method type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_client_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending client request by worker aggregation")
                        }
                    },
                    _ => panic!("unexpected pending client request by worker type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC => {
                saw_pending_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected pending server request aggregation"),
                    },
                    _ => panic!("unexpected pending server request type"),
                }
            }
            name if name == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC => {
                saw_answered_but_unresolved_server_requests = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => {
                            panic!("unexpected answered-but-unresolved server request aggregation")
                        }
                    },
                    _ => panic!("unexpected answered-but-unresolved server request type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC => {
                saw_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected server request backlog aggregation"),
                    },
                    _ => panic!("unexpected server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC => {
                saw_max_server_request_backlog = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 7.0);
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
                                    "outcome".to_string(),
                                    "downstream_session_ended".to_string(),
                                )])
                            );
                        }
                        _ => panic!("unexpected max server request backlog aggregation"),
                    },
                    _ => panic!("unexpected max server request backlog type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            pending_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => {
                            panic!("unexpected pending server request by worker aggregation")
                        }
                    },
                    _ => panic!("unexpected pending server request by worker type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC =>
            {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            answered_but_unresolved_server_request_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by worker aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by worker type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC => {
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            server_request_backlog_worker_points.extend(
                                histogram.data_points().map(|point| {
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
                                }),
                            );
                        }
                        _ => panic!("unexpected server request backlog by worker aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by worker type"),
                }
            }
            name if name == V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC => {
                saw_pending_server_requests_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 2.0);
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
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected pending server request by method aggregation"),
                    },
                    _ => panic!("unexpected pending server request by method type"),
                }
            }
            name if name
                == V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC =>
            {
                saw_answered_but_unresolved_server_requests_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 1.0);
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
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by method aggregation"
                        ),
                    },
                    _ => panic!("unexpected answered-but-unresolved server request by method type"),
                }
            }
            name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC => {
                saw_server_request_backlog_by_method = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
                            assert_eq!(point.sum(), 3.0);
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
                                        "outcome".to_string(),
                                        "downstream_session_ended".to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/tool/requestUserInput".to_string(),
                                    ),
                                ])
                            );
                        }
                        _ => panic!("unexpected server request backlog by method aggregation"),
                    },
                    _ => panic!("unexpected server request backlog by method type"),
                }
            }
            _ => {}
        }
    }

    assert!(saw_count);
    assert!(saw_duration);
    assert!(saw_pending_client_requests);
    assert!(saw_max_pending_client_requests);
    pending_client_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "2".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "7".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    pending_client_request_method_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_client_request_method_points,
        vec![
            (
                BTreeMap::from([
                    ("method".to_string(), "command/exec".to_string(),),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    ("method".to_string(), "thread/read".to_string(),),
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
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
    pending_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pending_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                0.0,
            ),
        ]
    );
    answered_but_unresolved_server_request_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        answered_but_unresolved_server_request_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                0.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    server_request_backlog_worker_points.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        server_request_backlog_worker_points,
        vec![
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "3".to_string()),
                ]),
                1,
                2.0,
            ),
            (
                BTreeMap::from([
                    (
                        "outcome".to_string(),
                        "downstream_session_ended".to_string(),
                    ),
                    ("worker_id".to_string(), "8".to_string()),
                ]),
                1,
                1.0,
            ),
        ]
    );
    assert!(saw_pending_server_requests_by_method);
    assert!(saw_answered_but_unresolved_server_requests_by_method);
    assert!(saw_server_request_backlog_by_method);
}

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

#[test]
fn records_v2_forwarded_notification_metrics_with_method_tags() {
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

    observability.record_v2_forwarded_notification("configWarning");

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
        if metric.name() == V2_FORWARDED_NOTIFICATION_COUNT_METRIC {
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
                            BTreeMap::from([("method".to_string(), "configWarning".to_string()),])
                        );
                    }
                    _ => panic!("unexpected forwarded notification count aggregation"),
                },
                _ => panic!("unexpected forwarded notification count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.forwarded_notification_counts,
        vec![GatewayV2ForwardedNotificationCounts {
            method: "configWarning".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_forwarded_notification_method,
        Some("configWarning".to_string())
    );
    assert_eq!(health.last_forwarded_notification_at.is_some(), true);
}

#[test]
fn records_v2_notification_send_failure_metrics_with_method_and_outcome_tags() {
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

    observability.record_v2_notification_send_failure("warning", "client_send_timed_out");

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
        if metric.name() == V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC {
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
                                ("method".to_string(), "warning".to_string()),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected notification send failure count aggregation"),
                },
                _ => panic!("unexpected notification send failure count type"),
            }
        }
    }

    assert!(saw_count);
    let health = observability.v2_connection_health.snapshot();
    assert_eq!(
        health.notification_send_failure_counts,
        vec![GatewayV2NotificationSendFailureCounts {
            method: "warning".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        health.last_notification_send_failure_method,
        Some("warning".to_string())
    );
    assert_eq!(
        health.last_notification_send_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(health.last_notification_send_failure_at.is_some(), true);
}

#[test]
fn records_v2_server_request_forward_send_failure_metrics_with_method_and_outcome_tags() {
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

    observability.record_v2_server_request_forward_send_failure(
        "item/commandExecution/requestApproval",
        "client_send_timed_out",
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
        if metric.name() == V2_SERVER_REQUEST_FORWARD_SEND_FAILURE_COUNT_METRIC {
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
                                    "item/commandExecution/requestApproval".to_string()
                                ),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                    }
                    _ => {
                        panic!("unexpected server-request forward send failure aggregation")
                    }
                },
                _ => panic!("unexpected server-request forward send failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_server_request_answer_delivery_failure_metrics_with_response_kind_tags() {
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

    observability.record_v2_server_request_answer_delivery_failure("response");

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
        if metric.name() == V2_SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC {
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
                            BTreeMap::from(
                                [("response_kind".to_string(), "response".to_string()),]
                            )
                        );
                    }
                    _ => {
                        panic!("unexpected server-request answer delivery failure aggregation")
                    }
                },
                _ => panic!("unexpected server-request answer delivery failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_server_request_rejection_delivery_failure_metrics_with_method_tags() {
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

    observability.record_v2_server_request_rejection_delivery_failure("item/tool/requestUserInput");

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
        if metric.name() == V2_SERVER_REQUEST_REJECTION_DELIVERY_FAILURE_COUNT_METRIC {
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
                                "method".to_string(),
                                "item/tool/requestUserInput".to_string()
                            ),])
                        );
                    }
                    _ => panic!("unexpected server-request rejection delivery failure aggregation"),
                },
                _ => panic!("unexpected server-request rejection delivery failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_client_response_send_failure_metrics_with_method_and_outcome_tags() {
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

    observability.record_v2_client_response_send_failure("model/list", "client_send_timed_out");

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
        if metric.name() == V2_CLIENT_RESPONSE_SEND_FAILURE_COUNT_METRIC {
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
                                ("method".to_string(), "model/list".to_string()),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected client response send failure count aggregation"),
                },
                _ => panic!("unexpected client response send failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_downstream_shutdown_failure_metrics_with_outcome_tags() {
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

    observability.record_v2_downstream_shutdown_failure("client_send_timed_out");

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
        if metric.name() == V2_DOWNSTREAM_SHUTDOWN_FAILURE_COUNT_METRIC {
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
                                "outcome".to_string(),
                                "client_send_timed_out".to_string()
                            ),])
                        );
                    }
                    _ => panic!("unexpected downstream shutdown failure count aggregation"),
                },
                _ => panic!("unexpected downstream shutdown failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_close_frame_send_failure_metrics_with_code_and_outcome_tags() {
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

    observability.record_v2_close_frame_send_failure(1008, "client_send_timed_out");

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
        if metric.name() == V2_CLOSE_FRAME_SEND_FAILURE_COUNT_METRIC {
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
                                ("code".to_string(), "1008".to_string()),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected close frame send failure count aggregation"),
                },
                _ => panic!("unexpected close frame send failure count type"),
            }
        }
    }

    assert!(saw_count);
}

#[test]
fn records_v2_suppressed_notification_metrics_with_reason_tags() {
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

    observability.record_v2_suppressed_notification("skills/changed", "pending_refresh");

    let health = observability.v2_connection_health().snapshot();
    assert_eq!(health.suppressed_notification_counts.len(), 1);
    assert_eq!(
        health.suppressed_notification_counts[0].method,
        "skills/changed"
    );
    assert_eq!(
        health.suppressed_notification_counts[0].reason,
        "pending_refresh"
    );
    assert_eq!(health.suppressed_notification_counts[0].count, 1);
    assert_eq!(
        health.last_suppressed_notification_method,
        Some("skills/changed".to_string())
    );
    assert_eq!(
        health.last_suppressed_notification_reason,
        Some("pending_refresh".to_string())
    );
    assert_eq!(health.last_suppressed_notification_at.is_some(), true);

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
        if metric.name() == V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC {
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
                                ("method".to_string(), "skills/changed".to_string()),
                                ("reason".to_string(), "pending_refresh".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected suppressed notification count aggregation"),
                },
                _ => panic!("unexpected suppressed notification count type"),
            }
        }
    }

    assert!(saw_count);
}

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

#[derive(Clone, Default)]
struct SharedWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

struct SharedWriterGuard {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

impl Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer
            .lock()
            .expect("log buffer should lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn capture_logs(f: impl FnOnce()) -> String {
    let writer = SharedWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .without_time()
            .with_writer(writer.clone()),
    );
    tracing::subscriber::with_default(subscriber, f);

    let bytes = writer
        .buffer
        .lock()
        .expect("log buffer should lock")
        .clone();
    String::from_utf8(bytes).expect("log output should be utf8")
}
