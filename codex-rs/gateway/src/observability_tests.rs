use super::ACCOUNT_CAPACITY_EVENT_COUNT_METRIC;
use super::ACCOUNT_LEASE_EVENT_COUNT_METRIC;
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
use super::WORKER_POOL_INVENTORY_METRIC;
use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayAccountLeaseState;
use crate::api::GatewayAccountPoolEntry;
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
use crate::api::GatewayWorkerPoolSlot;
use crate::api::GatewayWorkerPoolSnapshot;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionCompletionCounts;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use std::collections::BTreeMap;
use std::time::Duration;

#[path = "observability_tests_tail.rs"]
mod observability_tests_tail;

#[path = "observability_tests_metrics.rs"]
mod observability_tests_metrics;

#[path = "observability_tests_notifications.rs"]
mod observability_tests_notifications;

#[path = "observability_tests_transport_failures.rs"]
mod observability_tests_transport_failures;

#[path = "observability_tests_routes.rs"]
mod observability_tests_routes;

#[path = "observability_tests_routing_diagnostics.rs"]
mod observability_tests_routing_diagnostics;

#[path = "observability_tests_connection.rs"]
mod observability_tests_connection;

#[path = "observability_tests_rejections.rs"]
mod observability_tests_rejections;

#[path = "observability_tests_worker_capacity.rs"]
mod observability_tests_worker_capacity;

#[path = "observability_tests_connection_logs.rs"]
mod observability_tests_connection_logs;

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
