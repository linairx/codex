use super::ACCOUNT_CAPACITY_EVENT_COUNT_METRIC;
use super::GatewayObservability;
use super::PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC;
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
use std::time::Duration;

#[path = "observability_tests_tail.rs"]
mod observability_tests_tail;

#[path = "observability_tests_metrics.rs"]
mod observability_tests_metrics;

#[path = "observability_tests_notifications.rs"]
mod observability_tests_notifications;

#[path = "observability_tests_routes.rs"]
mod observability_tests_routes;

#[path = "observability_tests_connection.rs"]
mod observability_tests_connection;

#[path = "observability_tests_request_metrics.rs"]
mod observability_tests_request_metrics;

#[path = "observability_tests_support.rs"]
mod observability_tests_support;

pub(crate) use self::observability_tests_support::capture_logs;

#[test]
fn records_request_metrics_with_route_and_status_tags() {
    observability_tests_request_metrics::records_request_metrics_with_route_and_status_tags();
}

#[test]
fn records_v2_request_metrics_with_method_and_outcome_tags() {
    observability_tests_request_metrics::records_v2_request_metrics_with_method_and_outcome_tags();
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
