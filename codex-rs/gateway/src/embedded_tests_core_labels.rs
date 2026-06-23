use super::*;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn unlabeled_remote_account_workers_ignores_single_worker_remote() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![GatewayRemoteWorkerConfig {
            websocket_url: "ws://worker-a.invalid".to_string(),
            auth_token: None,
            account_id: None,
        }],
    };

    assert_eq!(
        unlabeled_remote_account_workers(&remote_runtime),
        Vec::new()
    );
}

#[test]
fn unlabeled_remote_account_workers_ignores_fully_labeled_multi_worker_remote() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-b".to_string()),
            },
        ],
    };

    assert_eq!(
        unlabeled_remote_account_workers(&remote_runtime),
        Vec::new()
    );
}

#[test]
fn unlabeled_remote_account_workers_reports_missing_multi_worker_account_labels() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: None,
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-c.invalid".to_string(),

                auth_token: None,
                account_id: Some("   ".to_string()),
            },
        ],
    };

    assert_eq!(
        unlabeled_remote_account_workers(&remote_runtime),
        vec![
            UnlabeledRemoteAccountWorker {
                worker_id: 1,
                websocket_url: "ws://worker-b.invalid".to_string(),
            },
            UnlabeledRemoteAccountWorker {
                worker_id: 2,
                websocket_url: "ws://worker-c.invalid".to_string(),
            },
        ]
    );
}

#[test]
fn remote_account_label_summary_reports_rollout_guardrail_state() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: Some("   ".to_string()),
            },
        ],
    };

    let summary = remote_account_label_summary(&remote_runtime);

    assert!(!summary.complete);
    assert_eq!(summary.unlabeled_worker_count(), 1);
    assert_eq!(
        summary.unlabeled_workers,
        vec![UnlabeledRemoteAccountWorker {
            worker_id: 1,
            websocket_url: "ws://worker-b.invalid".to_string(),
        }]
    );
}

#[test]
fn warn_unlabeled_remote_account_workers_logs_route_identity() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: None,
            },
        ],
    };
    let account_label_summary = remote_account_label_summary(&remote_runtime);

    let logs = capture_logs(|| {
        warn_unlabeled_remote_account_workers(&remote_runtime, &account_label_summary);
    });

    assert!(logs.contains("gateway remote multi-worker account labels are incomplete"));
    assert!(logs.contains("total_worker_count=2"));
    assert!(logs.contains("unlabeled_worker_count=1"));
    assert!(logs.contains("unlabeled_worker_ids=[1]"));
    assert!(logs.contains("unlabeled_worker_websocket_urls=[\"ws://worker-b.invalid\"]"));
}

#[test]
fn log_complete_remote_account_workers_logs_route_identity() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-b".to_string()),
            },
        ],
    };
    let account_label_summary = remote_account_label_summary(&remote_runtime);

    let logs = capture_logs(|| {
        log_complete_remote_account_workers(&remote_runtime, &account_label_summary);
    });

    assert!(logs.contains("gateway remote multi-worker account labels are complete"));
    assert!(logs.contains("total_worker_count=2"));
    assert!(logs.contains("labeled_worker_ids=[0, 1]"));
    assert!(
        logs.contains(
            "labeled_worker_websocket_urls=[\"ws://worker-a.invalid\", \"ws://worker-b.invalid\"]"
        ),
        "{logs}"
    );
    assert!(
        logs.contains("labeled_worker_account_ids=[\"acct-a\", \"acct-b\"]"),
        "{logs}"
    );
}

#[test]
fn remote_account_workers_record_startup_label_metrics() {
    let exporter = opentelemetry_sdk::metrics::InMemoryMetricExporter::default();
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
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-a.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-b.invalid".to_string(),

                auth_token: None,
                account_id: None,
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://worker-c.invalid".to_string(),

                auth_token: None,
                account_id: Some("acct-c".to_string()),
            },
        ],
    };

    record_remote_account_worker_label_metrics(&observability, &remote_runtime);

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let mut label_events = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .filter(|metric| metric.name() == "gateway_remote_account_label_events")
        .flat_map(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::U64(
                opentelemetry_sdk::metrics::data::MetricData::Sum(sum),
            ) => sum
                .data_points()
                .map(|point| {
                    let attributes = point
                        .attributes()
                        .map(|attribute| {
                            (
                                attribute.key.as_str().to_string(),
                                attribute.value.as_str().to_string(),
                            )
                        })
                        .collect::<std::collections::BTreeMap<_, _>>();
                    (
                        attributes
                            .get("worker_id")
                            .expect("worker_id attribute")
                            .clone(),
                        attributes.get("event").expect("event attribute").clone(),
                        point.value(),
                    )
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    label_events.sort();

    assert_eq!(
        label_events,
        vec![
            ("0".to_string(), "labeled".to_string(), 1),
            ("1".to_string(), "unlabeled".to_string(), 1),
            ("2".to_string(), "labeled".to_string(), 1),
        ]
    );
}

#[test]
fn remote_account_workers_do_not_record_startup_label_metrics_for_single_worker() {
    let exporter = opentelemetry_sdk::metrics::InMemoryMetricExporter::default();
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
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![GatewayRemoteWorkerConfig {
            websocket_url: "ws://worker-b.invalid".to_string(),
            auth_token: None,
            account_id: None,
        }],
    };

    record_remote_account_worker_label_metrics(&observability, &remote_runtime);

    let resource_metrics = metrics.snapshot().expect("snapshot");
    let label_event_count = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .filter(|metric| metric.name() == "gateway_remote_account_label_events")
        .map(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::U64(
                opentelemetry_sdk::metrics::data::MetricData::Sum(sum),
            ) => sum.data_points().count(),
            _ => 0,
        })
        .sum::<usize>();

    assert_eq!(label_event_count, 0);
}

#[derive(Clone)]
struct SharedWriter {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

struct SharedWriterGuard {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard {
            buffer: self.buffer.clone(),
        }
    }
}

impl std::io::Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .lock()
            .expect("log buffer lock")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_logs(f: impl FnOnce()) -> String {
    let buffer = Arc::new(StdMutex::new(Vec::new()));
    let writer = SharedWriter {
        buffer: buffer.clone(),
    };
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false),
    );

    tracing::subscriber::with_default(subscriber, f);
    let bytes = buffer.lock().expect("log buffer lock").clone();
    String::from_utf8(bytes).expect("logs should be valid UTF-8")
}
