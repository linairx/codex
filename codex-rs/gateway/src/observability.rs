use crate::api::GatewayWorkerPoolSnapshot;
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

#[path = "observability_metrics.rs"]
mod observability_metrics;

const REQUEST_COUNT_METRIC: &str = "gateway_http_requests";
const REQUEST_DURATION_METRIC: &str = "gateway_http_request_duration";
const REMOTE_ACCOUNT_LABEL_EVENT_COUNT_METRIC: &str = "gateway_remote_account_label_events";
const PROJECT_WORKER_ROUTE_SELECTION_COUNT_METRIC: &str = "gateway_project_worker_route_selections";
const WORKER_POOL_INVENTORY_METRIC: &str = "gateway_worker_pool_inventory";
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
}

impl GatewayObservability {
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
#[path = "observability_tests.rs"]
mod tests;
