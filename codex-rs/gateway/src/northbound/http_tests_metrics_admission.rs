use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn metrics_route_returns_runtime_metrics_snapshot() {
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
    let app = router_with_observability(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
        GatewayObservability::new(Some(metrics), false),
        Arc::new(GatewayScopeRegistry::default()),
        None,
        GatewayV2Timeouts::default(),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&body).expect("metrics json");
    let includes_request_metric = body["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .filter_map(|metric| metric["name"].as_str())
        .any(|name| name == "gateway_http_requests");
    assert_eq!(includes_request_metric, true);
}

#[tokio::test]
async fn metrics_route_reports_unavailable_without_runtime_snapshot() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&body).expect("error json");
    assert_eq!(
        body["error"],
        "gateway metrics snapshot unavailable: metrics exporter is disabled"
    );
}

#[tokio::test]
async fn router_emits_request_metrics() {
    let exporter = InMemoryMetricExporter::default();
    let metrics = codex_otel::MetricsClient::new(codex_otel::MetricsConfig::in_memory(
        "test",
        "codex-gateway",
        env!("CARGO_PKG_VERSION"),
        exporter.clone(),
    ))
    .expect("metrics");
    let app = router_with_observability(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
        GatewayObservability::new(Some(metrics), false),
        Arc::new(GatewayScopeRegistry::default()),
        None,
        GatewayV2Timeouts::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let resource_metrics = exporter
        .get_finished_metrics()
        .expect("finished metrics")
        .into_iter()
        .last()
        .expect("latest metrics");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

    let mut saw_count = false;
    let mut saw_duration = false;
    for metric in metrics {
        match metric.name() {
            "gateway_http_requests" => {
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
            "gateway_http_request_duration" => {
                saw_duration = true;
                match metric.data() {
                    AggregatedMetrics::F64(data) => match data {
                        MetricData::Histogram(histogram) => {
                            let point = histogram.data_points().next().expect("histogram point");
                            assert_eq!(point.count(), 1);
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

#[tokio::test]
async fn router_rate_limits_requests_per_scope() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: Some(1),
            turn_start_quota_per_minute: None,
        }),
    );

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .header("x-codex-tenant-id", "tenant-a")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first_response.status(), StatusCode::OK);

    let limited_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .header("x-codex-tenant-id", "tenant-a")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(limited_response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited_response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .is_some(),
        true
    );

    let other_scope_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .header("x-codex-tenant-id", "tenant-b")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(other_scope_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn router_enforces_turn_start_quota() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::new(GatewayAdmissionConfig {
            request_rate_limit_per_minute: None,
            turn_start_quota_per_minute: Some(1),
        }),
    );

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-1/turns")
                .header("content-type", "application/json")
                .header("x-codex-tenant-id", "tenant-a")
                .body(Body::from(r#"{"input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first_response.status(), StatusCode::OK);

    let limited_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-1/turns")
                .header("content-type", "application/json")
                .header("x-codex-tenant-id", "tenant-a")
                .body(Body::from(r#"{"input":"hello again"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(limited_response.status(), StatusCode::TOO_MANY_REQUESTS);
}
