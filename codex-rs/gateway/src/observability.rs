use crate::event::GatewayEvent;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionCompletionCounts;
use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use axum::extract::MatchedPath;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use codex_otel::MetricsClient;
use codex_otel::OtelProvider;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const REQUEST_COUNT_METRIC: &str = "gateway_http_requests";
const REQUEST_DURATION_METRIC: &str = "gateway_http_request_duration";
const REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC: &str = "gateway_remote_account_label_events";
const PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC: &str = "gateway_project_worker_route_selections";
const ACCOUNT_CAPACITY_EVENT_COUNT_METRIC: &str = "gateway_account_capacity_events";
const SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC: &str =
    "gateway_server_request_answer_delivery_failures";
const SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC: &str = "gateway_server_request_lifecycle_events";
const V2_REQUEST_COUNT_METRIC: &str = "gateway_v2_requests";
const V2_REQUEST_DURATION_METRIC: &str = "gateway_v2_request_duration";
const V2_CONNECTION_COUNT_METRIC: &str = "gateway_v2_connections";
const V2_CONNECTION_DURATION_METRIC: &str = "gateway_v2_connection_duration";
const V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC: &str =
    "gateway_v2_connection_pending_client_requests";
const V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC: &str =
    "gateway_v2_connection_max_pending_client_requests";
const V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC: &str =
    "gateway_v2_connection_pending_client_requests_by_worker";
const V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC: &str =
    "gateway_v2_connection_pending_client_requests_by_method";
const V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC: &str =
    "gateway_v2_connection_pending_server_requests";
const V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC: &str =
    "gateway_v2_connection_answered_but_unresolved_server_requests";
const V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC: &str =
    "gateway_v2_connection_server_request_backlog";
const V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC: &str =
    "gateway_v2_connection_max_server_request_backlog";
const V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC: &str =
    "gateway_v2_connection_pending_server_requests_by_worker";
const V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC: &str =
    "gateway_v2_connection_answered_but_unresolved_server_requests_by_worker";
const V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC: &str =
    "gateway_v2_connection_server_request_backlog_by_worker";
const V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC: &str =
    "gateway_v2_connection_pending_server_requests_by_method";
const V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC: &str =
    "gateway_v2_connection_answered_but_unresolved_server_requests_by_method";
const V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC: &str =
    "gateway_v2_connection_server_request_backlog_by_method";
const V2_SERVER_REQUEST_REJECTION_COUNT_METRIC: &str = "gateway_v2_server_request_rejections";
const V2_CLIENT_REQUEST_REJECTION_COUNT_METRIC: &str = "gateway_v2_client_request_rejections";
const V2_WORKER_RECONNECT_COUNT_METRIC: &str = "gateway_v2_worker_reconnects";
const V2_FAIL_CLOSED_REQUEST_COUNT_METRIC: &str = "gateway_v2_fail_closed_requests";
const V2_UPSTREAM_REQUEST_FAILURE_COUNT_METRIC: &str = "gateway_v2_upstream_request_failures";
const V2_DOWNSTREAM_SHUTDOWN_FAILURE_COUNT_METRIC: &str = "gateway_v2_downstream_shutdown_failures";
const V2_CLIENT_RESPONSE_SEND_FAILURE_COUNT_METRIC: &str =
    "gateway_v2_client_response_send_failures";
const V2_CLOSE_FRAME_SEND_FAILURE_COUNT_METRIC: &str = "gateway_v2_close_frame_send_failures";
const V2_FORWARDED_NOTIFICATION_COUNT_METRIC: &str = "gateway_v2_forwarded_notifications";
const V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC: &str = "gateway_v2_notification_send_failures";
const V2_SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC: &str =
    "gateway_v2_server_request_answer_delivery_failures";
const V2_SERVER_REQUEST_FORWARD_SEND_FAILURE_COUNT_METRIC: &str =
    "gateway_v2_server_request_forward_send_failures";
const V2_SERVER_REQUEST_REJECTION_DELIVERY_FAILURE_COUNT_METRIC: &str =
    "gateway_v2_server_request_rejection_delivery_failures";
const V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC: &str = "gateway_v2_suppressed_notifications";
const V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC: &str =
    "gateway_v2_server_request_lifecycle_events";
const V2_PROTOCOL_VIOLATION_COUNT_METRIC: &str = "gateway_v2_protocol_violations";
const V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC: &str = "gateway_v2_downstream_backpressure_events";
const V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC: &str = "gateway_v2_client_send_timeouts";
const V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC: &str = "gateway_v2_thread_list_deduplications";
const V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC: &str = "gateway_v2_thread_route_recoveries";
const V2_DEGRADED_THREAD_DISCOVERY_COUNT_METRIC: &str = "gateway_v2_degraded_thread_discovery";
const V2_ACCOUNT_CAPACITY_EVENT_COUNT_METRIC: &str = "gateway_v2_account_capacity_events";

type StderrLogLayer = Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync + 'static>;

#[derive(Debug, Clone, Default)]
pub struct GatewayObservability {
    metrics: Option<MetricsClient>,
    audit_logs_enabled: bool,
    v2_connection_health: Arc<GatewayV2ConnectionHealthRegistry>,
    operator_events: Option<broadcast::Sender<GatewayEvent>>,
}

impl GatewayObservability {
    pub fn new(metrics: Option<MetricsClient>, audit_logs_enabled: bool) -> Self {
        Self {
            metrics,
            audit_logs_enabled,
            v2_connection_health: Arc::new(GatewayV2ConnectionHealthRegistry::default()),
            operator_events: None,
        }
    }

    pub fn from_otel(otel: Option<&OtelProvider>, audit_logs_enabled: bool) -> Self {
        Self::new(
            otel.and_then(OtelProvider::metrics).cloned(),
            audit_logs_enabled,
        )
    }

    pub fn with_operator_events(mut self, events: broadcast::Sender<GatewayEvent>) -> Self {
        self.operator_events = Some(events);
        self
    }

    pub(crate) fn publish_operator_event(&self, event: GatewayEvent) {
        if let Some(events) = &self.operator_events {
            let _ = events.send(event);
        }
    }

    fn record_http_request(&self, method: &str, route: &str, status_code: u16, duration: Duration) {
        let status = status_code.to_string();
        let status_class = status_class(status_code);
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [
            ("method", method),
            ("route", route),
            ("status", status.as_str()),
            ("status_class", status_class),
        ];

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(REQUEST_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway request count metric: {err}");
            }
            if let Err(err) = metrics.histogram(REQUEST_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway request duration metric: {err}");
            }
        }
    }

    pub(crate) fn record_account_capacity_event(&self, worker_id: usize, event: &str) {
        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(ACCOUNT_CAPACITY_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway account capacity event metric: {err}");
        }
    }

    pub(crate) fn record_remote_account_label_event(&self, worker_id: usize, event: &str) {
        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway remote account label event metric: {err}");
        }
    }

    pub(crate) fn record_project_worker_route_selected(
        &self,
        worker_id: usize,
        tenant_id: &str,
        project_id: &str,
        thread_id: &str,
    ) {
        let worker_id_for_health = worker_id;
        let worker_id = worker_id.to_string();
        let tags = [
            ("worker_id", worker_id.as_str()),
            ("tenant_id", tenant_id),
            ("project_id", project_id),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway project worker route selection metric: {err}");
        }

        self.v2_connection_health
            .record_project_worker_route_selected(
                worker_id_for_health,
                tenant_id,
                project_id,
                thread_id,
            );
    }

    pub(crate) fn record_server_request_answer_delivery_failure(&self, response_kind: &str) {
        let tags = [("response_kind", response_kind)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway server-request answer delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_server_request_lifecycle_event(&self, event: &str, method: &str) {
        let tags = [("event", event), ("method", method)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway server-request lifecycle metric: {err}");
        }
    }

    pub(crate) fn record_v2_request(&self, method: &str, outcome: &str, duration: Duration) {
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_request(method, outcome, duration);

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(V2_REQUEST_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway v2 request count metric: {err}");
            }
            if let Err(err) = metrics.histogram(V2_REQUEST_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway v2 request duration metric: {err}");
            }
        }
    }

    pub(crate) fn record_v2_connection(
        &self,
        outcome: &str,
        duration: Duration,
        pending_counts: GatewayV2ConnectionPendingCounts,
        max_pending_client_request_count: usize,
        max_server_request_backlog_count: usize,
    ) {
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [("outcome", outcome)];

        if let Some(metrics) = &self.metrics {
            if let Err(err) = metrics.counter(V2_CONNECTION_COUNT_METRIC, 1, &tags) {
                tracing::warn!("failed to record gateway v2 connection count metric: {err}");
            }
            if let Err(err) = metrics.histogram(V2_CONNECTION_DURATION_METRIC, duration_ms, &tags) {
                tracing::warn!("failed to record gateway v2 connection duration metric: {err}");
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_PENDING_CLIENT_REQUEST_METRIC,
                pending_counts
                    .pending_client_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection pending client request metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_MAX_PENDING_CLIENT_REQUEST_METRIC,
                max_pending_client_request_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection max pending client request metric: {err}"
                );
            }
            for counts in &pending_counts.pending_client_request_worker_counts {
                let worker_id = counts
                    .worker_id
                    .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
                let worker_tags = [("outcome", outcome), ("worker_id", worker_id.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_WORKER_METRIC,
                    counts.pending_client_request_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending client request by worker metric: {err}"
                    );
                }
            }
            for counts in &pending_counts.pending_client_request_method_counts {
                let method_tags = [("outcome", outcome), ("method", counts.method.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_CLIENT_REQUEST_BY_METHOD_METRIC,
                    counts.pending_client_request_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending client request by method metric: {err}"
                    );
                }
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC,
                pending_counts
                    .pending_server_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection pending server request metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC,
                pending_counts
                    .answered_but_unresolved_server_request_count
                    .min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection answered-but-unresolved server request metric: {err}"
                );
            }
            let server_request_backlog_count = pending_counts
                .pending_server_request_count
                .saturating_add(pending_counts.answered_but_unresolved_server_request_count);
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC,
                server_request_backlog_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection server request backlog metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_MAX_SERVER_REQUEST_BACKLOG_METRIC,
                max_server_request_backlog_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection max server request backlog metric: {err}"
                );
            }
            for counts in &pending_counts.server_request_backlog_worker_counts {
                let worker_id = counts
                    .worker_id
                    .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
                let worker_tags = [("outcome", outcome), ("worker_id", worker_id.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_SERVER_REQUEST_BY_WORKER_METRIC,
                    counts.pending_server_request_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending server request by worker metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_WORKER_METRIC,
                    counts
                        .answered_but_unresolved_server_request_count
                        .min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection answered-but-unresolved server request by worker metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_WORKER_METRIC,
                    counts.server_request_backlog_count.min(i64::MAX as usize) as i64,
                    &worker_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection server request backlog by worker metric: {err}"
                    );
                }
            }
            for counts in &pending_counts.server_request_backlog_method_counts {
                let method_tags = [("outcome", outcome), ("method", counts.method.as_str())];
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_PENDING_SERVER_REQUEST_BY_METHOD_METRIC,
                    counts.pending_server_request_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection pending server request by method metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_BY_METHOD_METRIC,
                    counts
                        .answered_but_unresolved_server_request_count
                        .min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection answered-but-unresolved server request by method metric: {err}"
                    );
                }
                if let Err(err) = metrics.histogram(
                    V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC,
                    counts.server_request_backlog_count.min(i64::MAX as usize) as i64,
                    &method_tags,
                ) {
                    tracing::warn!(
                        "failed to record gateway v2 connection server request backlog by method metric: {err}"
                    );
                }
            }
        }
    }

    pub(crate) fn record_v2_server_request_rejection(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_server_request_rejection(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SERVER_REQUEST_REJECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request rejection metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_request_rejection(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_client_request_rejection(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLIENT_REQUEST_REJECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 client-request rejection metric: {err}");
        }
    }

    pub(crate) fn record_v2_worker_reconnect(&self, worker_id: usize, outcome: &str) {
        self.v2_connection_health
            .record_worker_reconnect_event(worker_id, outcome);

        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("outcome", outcome)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_WORKER_RECONNECT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 worker reconnect metric: {err}");
        }
    }

    pub(crate) fn record_v2_fail_closed_request(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_fail_closed_request(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_FAIL_CLOSED_REQUEST_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 fail-closed request metric: {err}");
        }
    }

    pub(crate) fn record_v2_upstream_request_failure(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_upstream_request_failure(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_UPSTREAM_REQUEST_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 upstream request failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_response_send_failure(&self, method: &str, outcome: &str) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_client_response_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) =
                metrics.counter(V2_CLIENT_RESPONSE_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!(
                "failed to record gateway v2 client response send failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_downstream_shutdown_failure(&self, outcome: &str) {
        let tags = [("outcome", outcome)];

        self.v2_connection_health
            .record_downstream_shutdown_failure(outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DOWNSTREAM_SHUTDOWN_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 downstream shutdown failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_close_frame_send_failure(&self, code: u16, outcome: &str) {
        let code_label = code.to_string();
        let tags = [("code", code_label.as_str()), ("outcome", outcome)];

        self.v2_connection_health
            .record_close_frame_send_failure(code, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLOSE_FRAME_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 close frame send failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_suppressed_notification(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        self.v2_connection_health
            .record_suppressed_notification(method, reason);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 suppressed notification metric: {err}");
        }
    }

    pub(crate) fn record_v2_forwarded_notification(&self, method: &str) {
        let tags = [("method", method)];

        self.v2_connection_health
            .record_forwarded_notification(method);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_FORWARDED_NOTIFICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 forwarded notification metric: {err}");
        }
    }

    pub(crate) fn record_v2_notification_send_failure(&self, method: &str, outcome: &str) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_notification_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_NOTIFICATION_SEND_FAILURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 notification send failure metric: {err}");
        }
    }

    pub(crate) fn record_v2_server_request_forward_send_failure(
        &self,
        method: &str,
        outcome: &str,
    ) {
        let tags = [("method", method), ("outcome", outcome)];

        self.v2_connection_health
            .record_server_request_forward_send_failure(method, outcome);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_FORWARD_SEND_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request forward send failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_answer_delivery_failure(&self, response_kind: &str) {
        let tags = [("response_kind", response_kind)];

        self.v2_connection_health
            .record_server_request_answer_delivery_failure(response_kind);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_ANSWER_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request answer delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_rejection_delivery_failure(&self, method: &str) {
        let tags = [("method", method)];

        self.v2_connection_health
            .record_server_request_rejection_delivery_failure(method);

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(
                V2_SERVER_REQUEST_REJECTION_DELIVERY_FAILURE_COUNT_METRIC,
                1,
                &tags,
            )
        {
            tracing::warn!(
                "failed to record gateway v2 server-request rejection delivery failure metric: {err}"
            );
        }
    }

    pub(crate) fn record_v2_server_request_lifecycle_event(&self, event: &str, method: &str) {
        self.record_v2_server_request_lifecycle_events(event, method, 1);
    }

    pub(crate) fn record_v2_server_request_lifecycle_events(
        &self,
        event: &str,
        method: &str,
        count: i64,
    ) {
        if count <= 0 {
            return;
        }

        self.v2_connection_health
            .record_server_request_lifecycle_events(event, method, count as usize);

        let tags = [("event", event), ("method", method)];

        if let Some(metrics) = &self.metrics
            && let Err(err) =
                metrics.counter(V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC, count, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request lifecycle metric: {err}");
        }
    }

    pub(crate) fn record_v2_protocol_violation(&self, phase: &str, reason: &str) {
        self.v2_connection_health
            .record_protocol_violation(phase, reason);

        self.record_v2_protocol_violation_metric(phase, reason);
    }

    pub(crate) fn record_v2_worker_protocol_violation(
        &self,
        worker_id: Option<usize>,
        phase: &str,
        reason: &str,
    ) {
        self.v2_connection_health
            .record_protocol_violation_for_worker(phase, reason, worker_id);

        self.record_v2_protocol_violation_metric(phase, reason);
    }

    fn record_v2_protocol_violation_metric(&self, phase: &str, reason: &str) {
        let tags = [("phase", phase), ("reason", reason)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_PROTOCOL_VIOLATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 protocol violation metric: {err}");
        }
    }

    pub(crate) fn record_v2_downstream_backpressure(&self, worker_id: Option<usize>) {
        self.v2_connection_health
            .record_downstream_backpressure(worker_id);

        let worker_id =
            worker_id.map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
        let tags = [("worker_id", worker_id.as_str())];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 downstream backpressure metric: {err}");
        }
    }

    pub(crate) fn record_v2_client_send_timeout(&self) {
        self.v2_connection_health.record_client_send_timeout();

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC, 1, &[])
        {
            tracing::warn!("failed to record gateway v2 client send timeout metric: {err}");
        }
    }

    pub(crate) fn record_v2_thread_list_deduplication(&self, selected_worker_id: Option<usize>) {
        self.v2_connection_health
            .record_thread_list_deduplication(selected_worker_id);

        let selected_worker_id = selected_worker_id
            .map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
        let tags = [("selected_worker_id", selected_worker_id.as_str())];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 thread-list deduplication metric: {err}");
        }
    }

    pub(crate) fn record_v2_thread_route_recovery(&self, outcome: &str) {
        self.v2_connection_health
            .record_thread_route_recovery(outcome);

        let tags = [("outcome", outcome)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 thread route recovery metric: {err}");
        }
    }

    pub(crate) fn record_v2_degraded_thread_discovery(
        &self,
        method: &str,
        reconnect_backoff_active: bool,
    ) {
        self.v2_connection_health
            .record_degraded_thread_discovery(method, reconnect_backoff_active);

        let reconnect_backoff_active = if reconnect_backoff_active {
            "true"
        } else {
            "false"
        };
        let tags = [
            ("method", method),
            ("reconnect_backoff_active", reconnect_backoff_active),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_DEGRADED_THREAD_DISCOVERY_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 degraded thread discovery metric: {err}");
        }
    }

    pub(crate) fn record_v2_account_capacity_event(
        &self,
        worker_id: usize,
        event: &str,
        context: Option<&GatewayRequestContext>,
        reason: Option<&str>,
    ) {
        self.v2_connection_health.record_account_capacity_event(
            worker_id,
            event,
            context.map(|context| context.tenant_id.as_str()),
            context.and_then(|context| context.project_id.as_deref()),
            reason,
        );

        let worker_id = worker_id.to_string();
        let tags = [("worker_id", worker_id.as_str()), ("event", event)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_ACCOUNT_CAPACITY_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 account capacity event metric: {err}");
        }
    }

    pub fn v2_connection_health(&self) -> Arc<GatewayV2ConnectionHealthRegistry> {
        Arc::clone(&self.v2_connection_health)
    }

    fn emit_audit_log(
        &self,
        method: &str,
        route: &str,
        status_code: u16,
        duration: Duration,
        context: Option<&GatewayRequestContext>,
    ) {
        if !self.audit_logs_enabled {
            return;
        }

        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        let tenant_id = context
            .map(|context| context.tenant_id.as_str())
            .unwrap_or("invalid");
        let project_id = context.and_then(|context| context.project_id.as_deref());

        tracing::event!(
            target: "codex_gateway.audit",
            Level::INFO,
            method,
            route,
            status_code,
            duration_ms,
            tenant_id,
            project_id,
            "gateway request completed"
        );
    }

    pub(crate) fn emit_v2_audit_log(
        &self,
        method: &str,
        outcome: &str,
        duration: Duration,
        context: &GatewayRequestContext,
    ) {
        if !self.audit_logs_enabled {
            return;
        }

        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        let tenant_id = context.tenant_id.as_str();
        let project_id = context.project_id.as_deref();

        tracing::event!(
            target: "codex_gateway.audit",
            Level::INFO,
            method,
            outcome,
            duration_ms,
            tenant_id,
            project_id,
            "gateway v2 request completed"
        );
    }

    pub(crate) fn emit_v2_connection_audit_log(
        &self,
        outcome: &str,
        duration: Duration,
        context: &GatewayRequestContext,
        detail: Option<&str>,
        pending_counts: GatewayV2ConnectionPendingCounts,
        completion_counts: &GatewayV2ConnectionCompletionCounts,
    ) {
        if !self.audit_logs_enabled {
            return;
        }

        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        let tenant_id = context.tenant_id.as_str();
        let project_id = context.project_id.as_deref();
        let pending_client_request_worker_counts =
            &pending_counts.pending_client_request_worker_counts;
        let pending_client_request_method_counts =
            &pending_counts.pending_client_request_method_counts;
        let server_request_backlog_count = pending_counts
            .pending_server_request_count
            .saturating_add(pending_counts.answered_but_unresolved_server_request_count);
        let server_request_backlog_worker_counts =
            &pending_counts.server_request_backlog_worker_counts;
        let server_request_backlog_method_counts =
            &pending_counts.server_request_backlog_method_counts;

        tracing::event!(
            target: "codex_gateway.audit",
            Level::INFO,
            outcome,
            duration_ms,
            tenant_id,
            project_id,
            detail,
            pending_client_request_count = pending_counts.pending_client_request_count,
            max_pending_client_request_count = completion_counts.max_pending_client_request_count,
            pending_client_request_worker_counts = ?pending_client_request_worker_counts,
            pending_client_request_method_counts = ?pending_client_request_method_counts,
            pending_server_request_count = pending_counts.pending_server_request_count,
            answered_but_unresolved_server_request_count =
                pending_counts.answered_but_unresolved_server_request_count,
            server_request_backlog_count,
            max_server_request_backlog_count = completion_counts.max_server_request_backlog_count,
            server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
            server_request_backlog_method_counts = ?server_request_backlog_method_counts,
            "gateway v2 connection completed"
        );
    }

    pub(crate) fn emit_v2_connection_log(
        &self,
        outcome: &str,
        duration: Duration,
        context: &GatewayRequestContext,
        detail: Option<&str>,
        pending_counts: GatewayV2ConnectionPendingCounts,
        completion_counts: &GatewayV2ConnectionCompletionCounts,
    ) {
        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        let tenant_id = context.tenant_id.as_str();
        let project_id = context.project_id.as_deref();
        let pending_client_request_worker_counts =
            &pending_counts.pending_client_request_worker_counts;
        let pending_client_request_method_counts =
            &pending_counts.pending_client_request_method_counts;
        let server_request_backlog_count = pending_counts
            .pending_server_request_count
            .saturating_add(pending_counts.answered_but_unresolved_server_request_count);
        let server_request_backlog_worker_counts =
            &pending_counts.server_request_backlog_worker_counts;
        let server_request_backlog_method_counts =
            &pending_counts.server_request_backlog_method_counts;
        match (v2_connection_log_level(outcome), detail) {
            (Level::INFO, Some(detail)) => tracing::event!(
                target: "codex_gateway.v2",
                Level::INFO,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                detail,
                pending_client_request_count = pending_counts.pending_client_request_count,
                max_pending_client_request_count =
                    completion_counts.max_pending_client_request_count,
                pending_client_request_worker_counts = ?pending_client_request_worker_counts,
                pending_client_request_method_counts = ?pending_client_request_method_counts,
                pending_server_request_count = pending_counts.pending_server_request_count,
                answered_but_unresolved_server_request_count =
                    pending_counts.answered_but_unresolved_server_request_count,
                server_request_backlog_count,
                max_server_request_backlog_count =
                    completion_counts.max_server_request_backlog_count,
                server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
                server_request_backlog_method_counts = ?server_request_backlog_method_counts,
                "gateway v2 connection completed"
            ),
            (Level::INFO, None) => tracing::event!(
                target: "codex_gateway.v2",
                Level::INFO,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                pending_client_request_count = pending_counts.pending_client_request_count,
                max_pending_client_request_count =
                    completion_counts.max_pending_client_request_count,
                pending_client_request_worker_counts = ?pending_client_request_worker_counts,
                pending_client_request_method_counts = ?pending_client_request_method_counts,
                pending_server_request_count = pending_counts.pending_server_request_count,
                answered_but_unresolved_server_request_count =
                    pending_counts.answered_but_unresolved_server_request_count,
                server_request_backlog_count,
                max_server_request_backlog_count =
                    completion_counts.max_server_request_backlog_count,
                server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
                server_request_backlog_method_counts = ?server_request_backlog_method_counts,
                "gateway v2 connection completed"
            ),
            (Level::WARN, Some(detail)) => tracing::event!(
                target: "codex_gateway.v2",
                Level::WARN,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                detail,
                pending_client_request_count = pending_counts.pending_client_request_count,
                max_pending_client_request_count =
                    completion_counts.max_pending_client_request_count,
                pending_client_request_worker_counts = ?pending_client_request_worker_counts,
                pending_client_request_method_counts = ?pending_client_request_method_counts,
                pending_server_request_count = pending_counts.pending_server_request_count,
                answered_but_unresolved_server_request_count =
                    pending_counts.answered_but_unresolved_server_request_count,
                server_request_backlog_count,
                max_server_request_backlog_count =
                    completion_counts.max_server_request_backlog_count,
                server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
                server_request_backlog_method_counts = ?server_request_backlog_method_counts,
                "gateway v2 connection completed"
            ),
            (Level::WARN, None) => tracing::event!(
                target: "codex_gateway.v2",
                Level::WARN,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                pending_client_request_count = pending_counts.pending_client_request_count,
                max_pending_client_request_count =
                    completion_counts.max_pending_client_request_count,
                pending_client_request_worker_counts = ?pending_client_request_worker_counts,
                pending_client_request_method_counts = ?pending_client_request_method_counts,
                pending_server_request_count = pending_counts.pending_server_request_count,
                answered_but_unresolved_server_request_count =
                    pending_counts.answered_but_unresolved_server_request_count,
                server_request_backlog_count,
                max_server_request_backlog_count =
                    completion_counts.max_server_request_backlog_count,
                server_request_backlog_worker_counts = ?server_request_backlog_worker_counts,
                server_request_backlog_method_counts = ?server_request_backlog_method_counts,
                "gateway v2 connection completed"
            ),
            _ => unreachable!("v2 connection log level should stay within info/warn"),
        }
    }
}

pub async fn observe_http_request(
    State(observability): State<GatewayObservability>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let context = GatewayRequestContext::from_headers(request.headers()).ok();
    let method = request.method().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    observability.record_http_request(&method, &route, response.status().as_u16(), duration);
    observability.emit_audit_log(
        &method,
        &route,
        response.status().as_u16(),
        duration,
        context.as_ref(),
    );

    response
}

pub fn init_tracing(otel: Option<&OtelProvider>) {
    let stderr_fmt: StderrLogLayer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::from_default_env())
        .boxed();
    let otel_logger_layer = otel.and_then(|provider| provider.logger_layer());
    let otel_tracing_layer = otel.and_then(|provider| provider.tracing_layer());
    let _ = tracing_subscriber::registry()
        .with(stderr_fmt)
        .with(otel_logger_layer)
        .with(otel_tracing_layer)
        .try_init();
}

fn status_class(status_code: u16) -> &'static str {
    match status_code {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

fn v2_connection_log_level(outcome: &str) -> Level {
    match outcome {
        "client_closed" | "client_disconnected" => Level::INFO,
        _ => Level::WARN,
    }
}

#[cfg(test)]
mod tests {
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                server_request_backlog_method_counts: vec![
                    GatewayV2ServerRequestBacklogMethodCounts {
                        method: "item/tool/requestUserInput".to_string(),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                ],
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                            _ => panic!(
                                "unexpected answered-but-unresolved server request aggregation"
                            ),
                        },
                        _ => panic!("unexpected answered-but-unresolved server request type"),
                    }
                }
                name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_METRIC => {
                    saw_server_request_backlog = true;
                    match metric.data() {
                        AggregatedMetrics::F64(data) => match data {
                            MetricData::Histogram(histogram) => {
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by worker type"
                        ),
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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
                        _ => panic!(
                            "unexpected answered-but-unresolved server request by method type"
                        ),
                    }
                }
                name if name == V2_CONNECTION_SERVER_REQUEST_BACKLOG_BY_METHOD_METRIC => {
                    saw_server_request_backlog_by_method = true;
                    match metric.data() {
                        AggregatedMetrics::F64(data) => match data {
                            MetricData::Histogram(histogram) => {
                                let point =
                                    histogram.data_points().next().expect("histogram point");
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

        observability
            .record_v2_server_request_rejection("item/tool/requestUserInput", "pending_limit");
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
    fn records_project_worker_route_selection_metrics_with_worker_tenant_and_project_tags() {
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

        observability.record_project_worker_route_selected(7, "tenant-a", "project-a", "thread-a");

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
        assert!(health.last_project_worker_route_selected_at.is_some());
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
                                BTreeMap::from([(
                                    "method".to_string(),
                                    "configWarning".to_string()
                                ),])
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
                                BTreeMap::from([(
                                    "response_kind".to_string(),
                                    "response".to_string()
                                ),])
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

        observability
            .record_v2_server_request_rejection_delivery_failure("item/tool/requestUserInput");

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
                        _ => panic!(
                            "unexpected server-request rejection delivery failure aggregation"
                        ),
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
                                BTreeMap::from([(
                                    "selected_worker_id".to_string(),
                                    "4".to_string(),
                                )])
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
}
