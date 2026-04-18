use crate::scope::GatewayRequestContext;
use axum::extract::MatchedPath;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use codex_otel::MetricsClient;
use codex_otel::OtelProvider;
use std::time::Duration;
use std::time::Instant;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const REQUEST_COUNT_METRIC: &str = "gateway_http_requests";
const REQUEST_DURATION_METRIC: &str = "gateway_http_request_duration";

type StderrLogLayer = Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync + 'static>;

#[derive(Debug, Clone, Default)]
pub struct GatewayObservability {
    metrics: Option<MetricsClient>,
    audit_logs_enabled: bool,
}

impl GatewayObservability {
    pub fn new(metrics: Option<MetricsClient>, audit_logs_enabled: bool) -> Self {
        Self {
            metrics,
            audit_logs_enabled,
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

#[cfg(test)]
mod tests {
    use super::GatewayObservability;
    use super::REQUEST_COUNT_METRIC;
    use super::REQUEST_DURATION_METRIC;
    use opentelemetry_sdk::metrics::InMemoryMetricExporter;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::MetricData;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

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
                            }
                            _ => panic!("unexpected duration aggregation"),
                        },
                        _ => panic!("unexpected duration type"),
                    }
                }
                _ => {}
            }
        }

        assert_eq!(saw_count, true);
        assert_eq!(saw_duration, true);
    }
}
