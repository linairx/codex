use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
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
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const REQUEST_COUNT_METRIC: &str = "gateway_http_requests";
const REQUEST_DURATION_METRIC: &str = "gateway_http_request_duration";
const V2_REQUEST_COUNT_METRIC: &str = "gateway_v2_requests";
const V2_REQUEST_DURATION_METRIC: &str = "gateway_v2_request_duration";
const V2_CONNECTION_COUNT_METRIC: &str = "gateway_v2_connections";
const V2_CONNECTION_DURATION_METRIC: &str = "gateway_v2_connection_duration";
const V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC: &str =
    "gateway_v2_connection_pending_server_requests";
const V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC: &str =
    "gateway_v2_connection_answered_but_unresolved_server_requests";
const V2_SERVER_REQUEST_REJECTION_COUNT_METRIC: &str = "gateway_v2_server_request_rejections";
const V2_WORKER_RECONNECT_COUNT_METRIC: &str = "gateway_v2_worker_reconnects";
const V2_FAIL_CLOSED_REQUEST_COUNT_METRIC: &str = "gateway_v2_fail_closed_requests";
const V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC: &str = "gateway_v2_suppressed_notifications";
const V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC: &str =
    "gateway_v2_server_request_lifecycle_events";
const V2_PROTOCOL_VIOLATION_COUNT_METRIC: &str = "gateway_v2_protocol_violations";
const V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC: &str = "gateway_v2_downstream_backpressure_events";
const V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC: &str = "gateway_v2_client_send_timeouts";
const V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC: &str = "gateway_v2_thread_list_deduplications";
const V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC: &str = "gateway_v2_thread_route_recoveries";

type StderrLogLayer = Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync + 'static>;

#[derive(Debug, Clone, Default)]
pub struct GatewayObservability {
    metrics: Option<MetricsClient>,
    audit_logs_enabled: bool,
    v2_connection_health: Arc<GatewayV2ConnectionHealthRegistry>,
}

impl GatewayObservability {
    pub fn new(metrics: Option<MetricsClient>, audit_logs_enabled: bool) -> Self {
        Self {
            metrics,
            audit_logs_enabled,
            v2_connection_health: Arc::new(GatewayV2ConnectionHealthRegistry::default()),
        }
    }

    pub fn from_otel(otel: Option<&OtelProvider>, audit_logs_enabled: bool) -> Self {
        Self::new(
            otel.and_then(OtelProvider::metrics).cloned(),
            audit_logs_enabled,
        )
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

    pub(crate) fn record_v2_request(&self, method: &str, outcome: &str, duration: Duration) {
        let duration_ms = duration.as_millis().min(i64::MAX as u128) as i64;
        let tags = [("method", method), ("outcome", outcome)];

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
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
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
                V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC,
                pending_server_request_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection pending server request metric: {err}"
                );
            }
            if let Err(err) = metrics.histogram(
                V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC,
                answered_but_unresolved_server_request_count.min(i64::MAX as usize) as i64,
                &tags,
            ) {
                tracing::warn!(
                    "failed to record gateway v2 connection answered-but-unresolved server request metric: {err}"
                );
            }
        }
    }

    pub(crate) fn record_v2_server_request_rejection(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SERVER_REQUEST_REJECTION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request rejection metric: {err}");
        }
    }

    pub(crate) fn record_v2_worker_reconnect(&self, worker_id: usize, outcome: &str) {
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

    pub(crate) fn record_v2_suppressed_notification(&self, method: &str, reason: &str) {
        let tags = [("method", method), ("reason", reason)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 suppressed notification metric: {err}");
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

        let tags = [("event", event), ("method", method)];

        if let Some(metrics) = &self.metrics
            && let Err(err) =
                metrics.counter(V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC, count, &tags)
        {
            tracing::warn!("failed to record gateway v2 server-request lifecycle metric: {err}");
        }
    }

    pub(crate) fn record_v2_protocol_violation(&self, phase: &str, reason: &str) {
        let tags = [("phase", phase), ("reason", reason)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_PROTOCOL_VIOLATION_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 protocol violation metric: {err}");
        }
    }

    pub(crate) fn record_v2_downstream_backpressure(&self, worker_id: Option<usize>) {
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
        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC, 1, &[])
        {
            tracing::warn!("failed to record gateway v2 client send timeout metric: {err}");
        }
    }

    pub(crate) fn record_v2_thread_list_deduplication(&self, selected_worker_id: Option<usize>) {
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
        let tags = [("outcome", outcome)];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway v2 thread route recovery metric: {err}");
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
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
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
            outcome,
            duration_ms,
            tenant_id,
            project_id,
            detail,
            pending_server_request_count,
            answered_but_unresolved_server_request_count,
            "gateway v2 connection completed"
        );
    }

    pub(crate) fn emit_v2_connection_log(
        &self,
        outcome: &str,
        duration: Duration,
        context: &GatewayRequestContext,
        detail: Option<&str>,
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
    ) {
        let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        let tenant_id = context.tenant_id.as_str();
        let project_id = context.project_id.as_deref();
        match (v2_connection_log_level(outcome), detail) {
            (Level::INFO, Some(detail)) => tracing::event!(
                target: "codex_gateway.v2",
                Level::INFO,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                detail,
                pending_server_request_count,
                answered_but_unresolved_server_request_count,
                "gateway v2 connection completed"
            ),
            (Level::INFO, None) => tracing::event!(
                target: "codex_gateway.v2",
                Level::INFO,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                pending_server_request_count,
                answered_but_unresolved_server_request_count,
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
                pending_server_request_count,
                answered_but_unresolved_server_request_count,
                "gateway v2 connection completed"
            ),
            (Level::WARN, None) => tracing::event!(
                target: "codex_gateway.v2",
                Level::WARN,
                outcome,
                duration_ms,
                tenant_id,
                project_id,
                pending_server_request_count,
                answered_but_unresolved_server_request_count,
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
    use super::GatewayObservability;
    use super::REQUEST_COUNT_METRIC;
    use super::REQUEST_DURATION_METRIC;
    use super::V2_CLIENT_SEND_TIMEOUT_COUNT_METRIC;
    use super::V2_CONNECTION_ANSWERED_BUT_UNRESOLVED_SERVER_REQUEST_METRIC;
    use super::V2_CONNECTION_COUNT_METRIC;
    use super::V2_CONNECTION_DURATION_METRIC;
    use super::V2_CONNECTION_PENDING_SERVER_REQUEST_METRIC;
    use super::V2_DOWNSTREAM_BACKPRESSURE_COUNT_METRIC;
    use super::V2_FAIL_CLOSED_REQUEST_COUNT_METRIC;
    use super::V2_PROTOCOL_VIOLATION_COUNT_METRIC;
    use super::V2_REQUEST_COUNT_METRIC;
    use super::V2_REQUEST_DURATION_METRIC;
    use super::V2_SERVER_REQUEST_LIFECYCLE_EVENT_COUNT_METRIC;
    use super::V2_SERVER_REQUEST_REJECTION_COUNT_METRIC;
    use super::V2_SUPPRESSED_NOTIFICATION_COUNT_METRIC;
    use super::V2_THREAD_LIST_DEDUPLICATION_COUNT_METRIC;
    use super::V2_THREAD_ROUTE_RECOVERY_COUNT_METRIC;
    use super::V2_WORKER_RECONNECT_COUNT_METRIC;
    use crate::scope::GatewayRequestContext;
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
            2,
            1,
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
        let mut saw_pending_server_requests = false;
        let mut saw_answered_but_unresolved_server_requests = false;
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
                _ => {}
            }
        }

        assert!(saw_count);
        assert!(saw_duration);
        assert!(saw_pending_server_requests);
        assert!(saw_answered_but_unresolved_server_requests);
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
                2,
                1,
            );
        });

        assert!(logs.contains("INFO"));
        assert!(logs.contains("gateway v2 connection completed"));
        assert!(logs.contains("client_disconnected"));
        assert!(logs.contains("tenant-a"));
        assert!(logs.contains("project-a"));
        assert!(logs.contains("7"));
        assert!(logs.contains("pending_server_request_count=2"));
        assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
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
                3,
                2,
            );
        });

        assert!(logs.contains("WARN"));
        assert!(logs.contains("gateway v2 connection completed"));
        assert!(logs.contains("downstream_backpressure"));
        assert!(logs.contains("downstream app-server event stream lagged"));
        assert!(logs.contains("tenant-a"));
        assert!(logs.contains("11"));
        assert!(logs.contains("pending_server_request_count=3"));
        assert!(logs.contains("answered_but_unresolved_server_request_count=2"));
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
                4,
                3,
            );
        });

        assert!(logs.contains("codex_gateway.audit"));
        assert!(logs.contains("gateway v2 connection completed"));
        assert!(logs.contains("client_send_timed_out"));
        assert!(logs.contains("tenant-audit"));
        assert!(logs.contains("project-audit"));
        assert!(logs.contains("17"));
        assert!(logs.contains("gateway websocket send timed out"));
        assert!(logs.contains("pending_server_request_count=4"));
        assert!(logs.contains("answered_but_unresolved_server_request_count=3"));
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
