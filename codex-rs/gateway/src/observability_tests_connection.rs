use super::*;

#[path = "observability_tests_connection_client.rs"]
mod observability_tests_connection_client;

#[path = "observability_tests_connection_server_requests.rs"]
mod observability_tests_connection_server_requests;

fn record_sample_v2_connection() -> GatewayObservability {
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

    observability
}

fn metric_attributes(
    point: &opentelemetry_sdk::metrics::data::HistogramDataPoint<f64>,
) -> BTreeMap<String, String> {
    point
        .attributes()
        .map(|attribute| {
            (
                attribute.key.as_str().to_string(),
                attribute.value.as_str().to_string(),
            )
        })
        .collect()
}
