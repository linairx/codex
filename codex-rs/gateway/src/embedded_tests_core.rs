use super::*;
use pretty_assertions::assert_eq;
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TurnStreamingCoverage {
    pub(crate) saw_thread_active: bool,
    pub(crate) saw_turn_started: bool,
    pub(crate) saw_hook_started: bool,
    pub(crate) saw_item_started: bool,
    pub(crate) saw_agent_delta: bool,
    pub(crate) saw_reasoning_summary_delta: bool,
    pub(crate) saw_reasoning_text_delta: bool,
    pub(crate) saw_command_output_delta: bool,
    pub(crate) saw_file_change_delta: bool,
    pub(crate) saw_hook_completed: bool,
    pub(crate) saw_item_completed: bool,
    pub(crate) saw_turn_completed: bool,
}

pub(crate) fn expected_extended_turn_notifications() -> HashSet<&'static str> {
    HashSet::from([
        "context_compacted",
        "error",
        "guardian_review_completed",
        "guardian_review_started",
        "mcp_tool_call_progress",
        "model_rerouted",
        "plan_delta",
        "raw_response_item_completed",
        "reasoning_summary_part_added",
        "terminal_interaction",
        "thread_token_usage_updated",
        "turn_diff_updated",
        "turn_plan_updated",
    ])
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RealtimeStreamingCoverage {
    pub(crate) saw_started: bool,
    pub(crate) saw_item_added: bool,
    pub(crate) saw_output_audio_delta: bool,
    pub(crate) saw_transcript_delta: bool,
    pub(crate) saw_transcript_done: bool,
    pub(crate) saw_sdp: bool,
    pub(crate) saw_error: bool,
    pub(crate) saw_closed: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TurnControlCoverage {
    pub(crate) saw_thread_active: bool,
    pub(crate) saw_turn_started: bool,
    pub(crate) saw_agent_delta: bool,
    pub(crate) saw_turn_completed: bool,
    pub(crate) saw_thread_idle: bool,
}

#[derive(Clone)]
pub(crate) struct MultiConnectionBootstrapSetupConfig {
    pub(crate) worker_label: &'static str,
    pub(crate) requires_openai_auth: bool,
    pub(crate) rate_limits: Vec<(&'static str, &'static str, i32)>,
    pub(crate) models: Vec<(&'static str, &'static str, bool)>,
    pub(crate) apps: Vec<(&'static str, &'static str)>,
    pub(crate) mcp_status_names: Vec<&'static str>,
    pub(crate) shared_cwd: &'static str,
    pub(crate) unique_cwd: &'static str,
}

pub(crate) fn bootstrap_setup_realtime_voices(worker_label: &str) -> RealtimeVoicesList {
    match worker_label {
        "worker-a" => RealtimeVoicesList {
            v1: vec![RealtimeVoice::Juniper, RealtimeVoice::Maple],
            v2: vec![RealtimeVoice::Alloy],
            default_v1: RealtimeVoice::Juniper,
            default_v2: RealtimeVoice::Alloy,
        },
        "worker-b" => RealtimeVoicesList {
            v1: vec![RealtimeVoice::Maple, RealtimeVoice::Cove],
            v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
            default_v1: RealtimeVoice::Cove,
            default_v2: RealtimeVoice::Marin,
        },
        _ => RealtimeVoicesList::builtin(),
    }
}

pub(crate) fn bootstrap_setup_plugin_list_json(
    worker_label: &str,
    unique_cwd: &str,
) -> serde_json::Value {
    serde_json::json!({
        "marketplaces": [{
            "name": format!("{worker_label}-marketplace"),
            "path": format!("{unique_cwd}/marketplace.json"),
            "interface": {
                "displayName": format!("{worker_label} Marketplace"),
            },
            "plugins": [{
                "id": format!("{worker_label}-plugin@local"),
                "name": format!("{worker_label}-plugin"),
                "source": {
                    "type": "local",
                    "path": format!("{unique_cwd}/plugins/{worker_label}-plugin"),
                },
                "installed": false,
                "enabled": false,
                "installPolicy": "AVAILABLE",
                "authPolicy": "ON_USE",
                "interface": {
                    "displayName": format!("{worker_label} Plugin"),
                    "shortDescription": format!("Plugin from {worker_label}"),
                    "longDescription": null,
                    "developerName": null,
                    "category": null,
                    "capabilities": [],
                    "websiteUrl": null,
                    "privacyPolicyUrl": null,
                    "termsOfServiceUrl": null,
                    "defaultPrompt": null,
                    "brandColor": null,
                    "composerIcon": null,
                    "composerIconUrl": null,
                    "logo": null,
                    "logoUrl": null,
                    "screenshots": [],
                    "screenshotUrls": [],
                },
            }],
        }],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": [format!("{worker_label}-plugin@local")],
    })
}

#[tokio::test]
async fn embedded_gateway_environment_manager_preserves_remote_exec_server_url() {
    let environment_manager = gateway_environment_manager(
        &GatewayConfig {
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            ..GatewayConfig::default()
        },
        &Arg0DispatchPaths {
            codex_self_exe: Some(std::env::current_exe().expect("current exe")),
            ..Arg0DispatchPaths::default()
        },
    )
    .await
    .expect("environment manager");

    let environment = environment_manager
        .default_environment()
        .expect("default environment");
    assert_eq!(environment.exec_server_url(), Some("ws://127.0.0.1:9753"));
}

#[tokio::test]
async fn embedded_gateway_environment_manager_builds_local_environment() {
    let environment_manager = gateway_environment_manager(
        &GatewayConfig::default(),
        &Arg0DispatchPaths {
            codex_self_exe: Some(std::env::current_exe().expect("current exe")),
            ..Arg0DispatchPaths::default()
        },
    )
    .await
    .expect("environment manager");

    assert_eq!(environment_manager.default_environment_id(), Some("local"));
}

#[tokio::test]
async fn embedded_gateway_rejects_unreachable_exec_server_configuration() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");

    let result = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            exec_server_url: Some("ws://127.0.0.1:1".to_string()),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await;

    let err = match result {
        Ok(_server) => panic!("gateway startup should reject unreachable exec-server"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::Other);
    assert_eq!(
        err.to_string()
            .contains("failed to initialize remote exec-server environment"),
        true
    );
}

#[test]
fn embedded_gateway_execution_mode_tracks_exec_server_configuration() {
    assert_eq!(
        gateway_execution_mode(&GatewayConfig::default()),
        GatewayExecutionMode::InProcess
    );
    assert_eq!(
        gateway_execution_mode(&GatewayConfig {
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            ..GatewayConfig::default()
        }),
        GatewayExecutionMode::ExecServer
    );
}

#[tokio::test]
async fn remote_gateway_runtime_rejects_exec_server_url_configuration() {
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");

    let result = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: "ws://127.0.0.1:8081".to_string(),
                    auth_token: None,
                    account_id: None,
                }],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await;

    let err = match result {
        Ok(_server) => panic!("remote runtime should reject gateway exec server config"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "remote gateway runtime does not support `--exec-server-url`; configure execution on the remote app-server workers instead"
    );
}

#[tokio::test]
async fn remote_gateway_runtime_rejects_blank_worker_websocket_url() {
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");

    let result = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: "   ".to_string(),
                    auth_token: None,
                    account_id: None,
                }],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await;

    let err = match result {
        Ok(_server) => panic!("remote runtime should reject blank worker websocket URL"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "remote worker 0 websocket URL must not be blank"
    );
}

#[tokio::test]
async fn gateway_rejects_zero_v2_transport_hardening_values() {
    let bind_address = "127.0.0.1:0".parse().expect("bind address");
    let cases = vec![
        (
            GatewayConfig {
                bind_address,
                v2_initialize_timeout: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero initialize timeout",
            "gateway v2 initialize timeout must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_client_send_timeout: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero client send timeout",
            "gateway v2 client send timeout must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_reconnect_retry_backoff: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero reconnect retry backoff",
            "gateway v2 reconnect retry backoff must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_max_pending_server_requests: 0,
                ..GatewayConfig::default()
            },
            "gateway should reject zero pending server request limit",
            "gateway v2 max pending server requests must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_max_pending_client_requests: 0,
                ..GatewayConfig::default()
            },
            "gateway should reject zero pending client request limit",
            "gateway v2 max pending client requests must be greater than zero",
        ),
    ];

    for (gateway_config, panic_message, expected_message) in cases {
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");

        let result = start_gateway_server(
            gateway_config,
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await;

        let err = match result {
            Ok(_server) => panic!("{panic_message}"),
            Err(err) => err,
        };

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), expected_message);
    }
}

#[tokio::test]
async fn embedded_server_serves_thread_creation_requests() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("http response");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: ThreadResponse = response.json().await.expect("thread response");
    assert_eq!(body.thread.ephemeral, true);
    assert_eq!(body.thread.id.is_empty(), false);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_streams_thread_started_events_over_sse() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let mut events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread create response");
    let thread: ThreadResponse = create_response.json().await.expect("thread response");

    let event_body = timeout(std::time::Duration::from_secs(5), async {
        let mut body = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            body.push_str(std::str::from_utf8(&chunk).expect("utf8"));
            if body.contains("\n\n") {
                break body;
            }
        }
    })
    .await
    .expect("timed out waiting for SSE event");

    assert_eq!(event_body.contains("event: thread/started"), true);
    assert_eq!(event_body.contains(&thread.thread.id), true);

    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_scopes_threads_by_tenant_and_project_headers() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread create response");
    assert_eq!(create_response.status(), reqwest::StatusCode::OK);
    let thread: ThreadResponse = create_response.json().await.expect("thread response");

    let same_scope_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            thread.thread.id
        ))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-a")
        .send()
        .await
        .expect("same-scope read response");
    assert_eq!(same_scope_response.status(), reqwest::StatusCode::OK);

    let other_project_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            thread.thread.id
        ))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-b")
        .send()
        .await
        .expect("other-project read response");
    assert_eq!(
        other_project_response.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    let other_tenant_list = client
        .get(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-b")
        .header("x-codex-project-id", "project-a")
        .send()
        .await
        .expect("other-tenant list response");
    assert_eq!(other_tenant_list.status(), reqwest::StatusCode::OK);
    let body: ListThreadsResponse = other_tenant_list.json().await.expect("list response");
    assert_eq!(body.data.is_empty(), true);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_applies_per_scope_request_rate_limits() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            request_rate_limit_per_minute: Some(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);

    let limited_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("limited response");
    assert_eq!(
        limited_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        limited_response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .is_some(),
        true
    );

    let other_scope_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-b")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("other scope response");
    assert_eq!(other_scope_response.status(), reqwest::StatusCode::OK);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_bootstrap_and_thread_workflow() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to embedded gateway");

    let account: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read should succeed through embedded gateway");
    assert_eq!(account.account, None);

    let models: ModelListResponse = client
        .request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(2),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        })
        .await
        .expect("model/list should succeed through embedded gateway");
    assert_eq!(models.data.is_empty(), false);

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some(codex_home.path().display().to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");
    assert_eq!(started.thread.ephemeral, false);
    assert_eq!(started.thread.id.is_empty(), false);

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(4),
            params: ThreadListParams {
                parent_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        })
        .await
        .expect("thread/list should succeed through embedded gateway");
    assert_eq!(listed.next_cursor, None);

    let loaded: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(5),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should succeed through embedded gateway");
    assert_eq!(loaded.data.contains(&started.thread.id), true);

    let read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed through embedded gateway");
    let mut expected_thread = started.thread.clone();
    expected_thread.created_at = read.thread.created_at;
    expected_thread.updated_at = read.thread.updated_at;
    expected_thread.recency_at = read.thread.recency_at;
    assert_eq!(read.thread, expected_thread);

    let renamed_thread_name = "Gateway Embedded Thread".to_string();
    let rename_response: ThreadSetNameResponse = client
        .request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(7),
            params: ThreadSetNameParams {
                thread_id: started.thread.id.clone(),
                name: renamed_thread_name.clone(),
            },
        })
        .await
        .expect("thread/name/set should succeed through embedded gateway");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let rename_notification = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
            {
                break notification;
            }
        }
    })
    .await
    .expect("thread/name/updated notification should arrive");
    assert_eq!(
        rename_notification.thread_name,
        Some(renamed_thread_name.clone())
    );

    let renamed: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read after rename should succeed through embedded gateway");
    assert_eq!(renamed.thread.name, Some(renamed_thread_name.clone()));

    let memory_mode_response: ThreadMemoryModeSetResponse = client
        .request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(9),
            params: ThreadMemoryModeSetParams {
                thread_id: started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("thread/memoryMode/set should succeed through embedded gateway");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[test]
fn embedded_server_supports_v2_thread_reentry_from_later_client_session() {
    const TEST_STACK_SIZE_BYTES: usize = 32 * 1024 * 1024;

    let handle = std::thread::Builder::new()
        .name("embedded_server_supports_v2_thread_reentry_from_later_client_session".to_string())
        .stack_size(TEST_STACK_SIZE_BYTES)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime should build");
            runtime.block_on(async {
                let model_server = start_mock_responses_server_repeating_assistant("Done").await;
                let codex_home = tempdir().expect("tempdir");
                write_mock_responses_config_toml(codex_home.path(), &model_server.uri)
                    .expect("config.toml should be written");
                let config = Config::load_default_with_cli_overrides_for_codex_home(
                    codex_home.path().to_path_buf(),
                    Vec::new(),
                )
                .await
                .expect("config");
                let server = start_embedded_gateway_server(
                    GatewayConfig {
                        bind_address: "127.0.0.1:0".parse().expect("bind address"),
                        enable_codex_api_key_env: false,
                        session_source: SessionSource::Cli,
                        ..GatewayConfig::default()
                    },
                    Arg0DispatchPaths::default(),
                    config,
                    Vec::new(),
                    LoaderOverrides::default(),
                )
                .await
                .expect("server");

                let mut owner_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: format!("ws://{}/", server.local_addr()),
                        auth_token: None,
                    },
                    client_name: "codex-gateway-test".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: true,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 8,
                })
                .await
                .expect("owner client should connect to embedded gateway");

                let started: AppServerThreadStartResponse = owner_client
                    .request_typed(ClientRequest::ThreadStart {
                        request_id: RequestId::Integer(1),
                        params: ThreadStartParams {
                            cwd: Some(codex_home.path().display().to_string()),
                            ephemeral: Some(false),
                            ..Default::default()
                        },
                    })
                    .await
                    .expect("thread/start should succeed through embedded gateway");

                let turn_started_response: TurnStartResponse = owner_client
                    .request_typed(ClientRequest::TurnStart {
                        request_id: RequestId::Integer(2),
                        params: TurnStartParams {
                            thread_id: started.thread.id.clone(),
                            input: vec![UserInput::Text {
                                text: "materialize thread history".to_string(),
                                text_elements: Vec::new(),
                            }],
                            responsesapi_client_metadata: None,
                            cwd: None,
                            approval_policy: None,
                            approvals_reviewer: None,
                            sandbox_policy: None,
                            model: None,
                            service_tier: None,
                            effort: None,
                            summary: None,
                            personality: None,
                            output_schema: None,
                            collaboration_mode: None,
                            ..TurnStartParams::default()
                        },
                    })
                    .await
                    .expect("turn/start should succeed through embedded gateway");
                let turn_id = turn_started_response.turn.id.clone();

                timeout(Duration::from_secs(10), async {
                    loop {
                        let event = owner_client
                            .next_event()
                            .await
                            .expect("event stream should stay open");
                        if let AppServerEvent::ServerNotification(
                            ServerNotification::TurnCompleted(TurnCompletedNotification {
                                thread_id,
                                turn,
                            }),
                        ) = event
                            && thread_id == started.thread.id
                            && turn.id == turn_id
                            && turn.status == TurnStatus::Completed
                        {
                            break;
                        }
                    }
                })
                .await
                .expect("turn completion should arrive");

                assert_remote_client_shutdown(owner_client.shutdown().await);

                let reentry_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: format!("ws://{}/", server.local_addr()),
                        auth_token: None,
                    },
                    client_name: "codex-gateway-test".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: true,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 8,
                })
                .await
                .expect("reentry client should connect to embedded gateway");

                let resumed: ThreadResumeResponse = reentry_client
                    .request_typed(ClientRequest::ThreadResume {
                        request_id: RequestId::Integer(3),
                        params: ThreadResumeParams {
                            thread_id: started.thread.id.clone(),
                            history: None,
                            path: None,
                            model: None,
                            model_provider: None,
                            service_tier: None,
                            cwd: None,
                            approval_policy: None,
                            approvals_reviewer: None,
                            sandbox: None,
                            config: None,
                            base_instructions: None,
                            developer_instructions: None,
                            personality: None,
                            ..Default::default()
                        },
                    })
                    .await
                    .expect("thread/resume should succeed for later embedded client");
                assert_eq!(resumed.thread.id, started.thread.id);

                let read: AppServerThreadReadResponse = reentry_client
                    .request_typed(ClientRequest::ThreadRead {
                        request_id: RequestId::Integer(4),
                        params: ThreadReadParams {
                            thread_id: started.thread.id.clone(),
                            include_turns: false,
                        },
                    })
                    .await
                    .expect("thread/read should succeed for later embedded client");
                assert_eq!(read.thread.id, started.thread.id);

                assert_remote_client_shutdown(reentry_client.shutdown().await);
                server.shutdown().await.expect("shutdown");
                model_server.shutdown().await;
            });
        })
        .expect("test thread should spawn");

    handle.join().expect("test thread should not panic");
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_bootstrap_setup_methods() {
    let codex_home = tempdir().expect("tempdir");
    write_embedded_plugin_fixture(codex_home.path(), "http://127.0.0.1:1");
    std::fs::write(
        codex_home.path().join("config.toml"),
        "model = \"gpt-5\"\n\n[features]\nplugins = true\n",
    )
    .expect("config.toml should be written");
    let fuzzy_root = tempdir().expect("fuzzy search root tempdir");
    std::fs::write(fuzzy_root.path().join("gateway-alpha.txt"), "x")
        .expect("fuzzy search fixture should be written");
    let fs_root = tempdir().expect("filesystem operation root tempdir");
    let git_root = tempdir().expect("git diff root tempdir");
    let git_repo = git_root.path().join("repo");
    std::fs::create_dir(&git_repo).expect("git repo directory should be created");
    Command::new("git")
        .args(["init"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git init should run");
    Command::new("git")
        .args(["config", "user.name", "Gateway Test"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git user.name should be configured");
    Command::new("git")
        .args(["config", "user.email", "gateway@example.com"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git user.email should be configured");
    std::fs::write(git_repo.join("tracked.txt"), "base\n")
        .expect("tracked git fixture should be written");
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git add should run");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git commit should run");
    let git_remote = git_root.path().join("remote.git");
    Command::new("git")
        .args([
            "init",
            "--bare",
            git_remote
                .to_str()
                .expect("git remote path should be valid utf8"),
        ])
        .output()
        .await
        .expect("git bare remote init should run");
    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            git_remote
                .to_str()
                .expect("git remote path should be valid utf8"),
        ])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git remote add should run");
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git branch should be read");
    let branch = String::from_utf8(branch_output.stdout)
        .expect("git branch should be utf8")
        .trim()
        .to_string();
    Command::new("git")
        .args(["push", "-u", "origin", &branch])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git push should run");
    let remote_sha_output = Command::new("git")
        .args(["rev-parse", &format!("origin/{branch}")])
        .current_dir(&git_repo)
        .output()
        .await
        .expect("git remote sha should be read");
    let remote_sha = String::from_utf8(remote_sha_output.stdout)
        .expect("git remote sha should be utf8")
        .trim()
        .to_string();
    std::fs::write(git_repo.join("tracked.txt"), "changed\n")
        .expect("tracked git fixture should be modified");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to embedded gateway");

    let detected: ExternalAgentConfigDetectResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigDetectParams {
                include_home: false,
                cwds: Some(vec![codex_home.path().to_path_buf()]),
            },
        })
        .await
        .expect("externalAgentConfig/detect should succeed through embedded gateway");
    assert_eq!(detected.items.is_empty(), true);

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(2),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should succeed through embedded gateway");
    assert!(!imported.import_id.is_empty());

    let config_read: ConfigReadResponse = client
        .request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(3),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some(codex_home.path().display().to_string()),
            },
        })
        .await
        .expect("config/read should succeed through embedded gateway");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5"));
    assert_eq!(config_read.layers.is_some(), true);

    let config_requirements: ConfigRequirementsReadResponse = client
        .request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(4),
            params: None,
        })
        .await
        .expect("configRequirements/read should succeed through embedded gateway");
    assert_eq!(config_requirements.requirements.is_none(), true);

    let experimental_features: ExperimentalFeatureListResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(5),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(20),
                ..Default::default()
            },
        })
        .await
        .expect("experimentalFeature/list should succeed through embedded gateway");
    assert_eq!(experimental_features.data.is_empty(), false);

    let collaboration_modes: CollaborationModeListResponse = client
        .request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(6),
            params: CollaborationModeListParams::default(),
        })
        .await
        .expect("collaborationMode/list should succeed through embedded gateway");
    assert_eq!(collaboration_modes.data.is_empty(), false);

    let fs_dir = fs_root.path().join("nested");
    let create_directory: FsCreateDirectoryResponse = client
        .request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(29),
            params: FsCreateDirectoryParams {
                path: fs_dir
                    .clone()
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        })
        .await
        .expect("fs/createDirectory should succeed through embedded gateway");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let fs_file = fs_dir.join("gateway.txt");
    let write_file: FsWriteFileResponse = client
        .request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(30),
            params: FsWriteFileParams {
                path: fs_file
                    .clone()
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "Z2F0ZXdheS1lbWJlZGRlZA==".to_string(),
            },
        })
        .await
        .expect("fs/writeFile should succeed through embedded gateway");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = client
        .request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(31),
            params: FsReadFileParams {
                path: fs_file
                    .clone()
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        })
        .await
        .expect("fs/readFile should succeed through embedded gateway");
    assert_eq!(read_file.data_base64, "Z2F0ZXdheS1lbWJlZGRlZA==");

    let metadata: FsGetMetadataResponse = client
        .request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(32),
            params: FsGetMetadataParams {
                path: fs_file
                    .clone()
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        })
        .await
        .expect("fs/getMetadata should succeed through embedded gateway");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = client
        .request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(33),
            params: FsReadDirectoryParams {
                path: fs_dir
                    .clone()
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        })
        .await
        .expect("fs/readDirectory should succeed through embedded gateway");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");
    assert_eq!(directory.entries[0].is_file, true);

    let copied_file = fs_dir.join("gateway-copy.txt");
    let copy: FsCopyResponse = client
        .request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(34),
            params: FsCopyParams {
                source_path: fs_file
                    .clone()
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: copied_file
                    .clone()
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        })
        .await
        .expect("fs/copy should succeed through embedded gateway");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = client
        .request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(35),
            params: FsRemoveParams {
                path: copied_file
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(true),
                force: Some(true),
            },
        })
        .await
        .expect("fs/remove should succeed through embedded gateway");
    assert_eq!(remove, FsRemoveResponse {});

    let fuzzy_search: FuzzyFileSearchResponse = client
        .request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(36),
            params: FuzzyFileSearchParams {
                query: "gate".to_string(),
                roots: vec![fuzzy_root.path().display().to_string()],
                cancellation_token: Some("embedded-fuzzy-search".to_string()),
            },
        })
        .await
        .expect("fuzzyFileSearch should succeed through embedded gateway");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].path, "gateway-alpha.txt");
    assert_eq!(fuzzy_search.files[0].file_name, "gateway-alpha.txt");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let fuzzy_root_path = fuzzy_root.path().display().to_string();
    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(37),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "embedded-fuzzy-session".to_string(),
                roots: vec![fuzzy_root_path.clone()],
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStart should succeed through embedded gateway");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(38),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "embedded-fuzzy-session".to_string(),
                query: "gate".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionUpdate should succeed through embedded gateway");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "embedded-fuzzy-session"
                && notification.query == "gate"
                && !notification.files.is_empty()
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated notification should arrive");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(fuzzy_session_updated.files[0].root, fuzzy_root_path);
    assert_eq!(fuzzy_session_updated.files[0].path, "gateway-alpha.txt");

    let fuzzy_session_completed = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "embedded-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted notification should arrive");
    assert_eq!(fuzzy_session_completed.session_id, "embedded-fuzzy-session");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = client
        .request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(39),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "embedded-fuzzy-session".to_string(),
            },
        })
        .await
        .expect("fuzzyFileSearch/sessionStop should succeed through embedded gateway");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let windows_setup: WindowsSandboxSetupStartResponse = client
        .request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(40),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    codex_home
                        .path()
                        .to_path_buf()
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        })
        .await
        .expect("windowsSandbox/setupStart should succeed through embedded gateway");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted notification should arrive");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );

    let skills: SkillsListResponse = client
        .request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(7),
            params: SkillsListParams {
                cwds: vec![codex_home.path().to_path_buf()],
                force_reload: false,
            },
        })
        .await
        .expect("skills/list should succeed through embedded gateway");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(skills.data[0].cwd, codex_home.path().to_path_buf());

    let apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(8),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        })
        .await
        .expect("app/list should succeed through embedded gateway");
    assert_eq!(apps.next_cursor, None);
    assert!(apps.data.is_empty());

    let mcp_statuses: ListMcpServerStatusResponse = client
        .request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(9),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: Some(10),
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        })
        .await
        .expect("mcpServerStatus/list should succeed through embedded gateway");
    assert_eq!(mcp_statuses.next_cursor, None);
    assert!(mcp_statuses.data.is_empty());

    let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
        "cwds": [codex_home.path()],
    }))
    .expect("plugin/list params should deserialize");
    let plugins: PluginListResponse = client
        .request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(10),
            params: plugin_list_params,
        })
        .await
        .expect("plugin/list should succeed through embedded gateway");
    assert_eq!(plugins.marketplaces.len(), 1);
    assert_eq!(plugins.marketplaces[0].name, "local");
    assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
    assert_eq!(plugins.marketplaces[0].plugins[0].name, "demo-plugin");
    assert_eq!(plugins.marketplaces[0].plugins[0].installed, false);

    let plugin_read_params: PluginReadParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": codex_home.path().join(".agents/plugins/marketplace.json"),
        "pluginName": "demo-plugin",
    }))
    .expect("plugin/read params should deserialize");
    let plugin: PluginReadResponse = client
        .request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(11),
            params: plugin_read_params,
        })
        .await
        .expect("plugin/read should succeed through embedded gateway");
    assert_eq!(plugin.plugin.summary.name, "demo-plugin");
    assert_eq!(plugin.plugin.marketplace_name, "local");
    assert_eq!(
        plugin.plugin.description.as_deref(),
        Some("Demo plugin detail")
    );

    let plugin_install_params: PluginInstallParams = serde_json::from_value(serde_json::json!({
        "marketplacePath": codex_home.path().join(".agents/plugins/marketplace.json"),
        "pluginName": "demo-plugin",
    }))
    .expect("plugin/install params should deserialize");
    let install: PluginInstallResponse = client
        .request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(12),
            params: plugin_install_params,
        })
        .await
        .expect("plugin/install should succeed through embedded gateway");
    assert_eq!(
        install,
        PluginInstallResponse {
            auth_policy: PluginAuthPolicy::OnInstall,
            apps_needing_auth: Vec::new(),
        }
    );
    let installed_plugin_root = codex_home
        .path()
        .join("plugins/cache/local/demo-plugin/local");
    assert_eq!(installed_plugin_root.exists(), true);

    let uninstall: PluginUninstallResponse = client
        .request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(14),
            params: PluginUninstallParams {
                plugin_id: "demo-plugin@local".to_string(),
            },
        })
        .await
        .expect("plugin/uninstall should succeed through embedded gateway");
    assert_eq!(uninstall, PluginUninstallResponse {});
    assert_eq!(installed_plugin_root.exists(), false);

    let batch_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(15),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some(codex_home.path().join("config.toml").display().to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite should succeed through embedded gateway");
    assert_eq!(batch_write.version.is_empty(), false);

    let config_value_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(16),
            params: ConfigValueWriteParams {
                key_path: "plugins.demo-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        })
        .await
        .expect("config/value/write should succeed through embedded gateway");
    assert_eq!(config_value_write.version.is_empty(), false);
    assert!(
        std::fs::read_to_string(codex_home.path().join("config.toml"))
            .expect("config.toml should remain readable")
            .contains("demo-plugin"),
        "config/value/write should persist plugin config"
    );

    let watch: FsWatchResponse = client
        .request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(17),
            params: FsWatchParams {
                watch_id: "embedded-watch".to_string(),
                path: codex_home
                    .path()
                    .join("config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        })
        .await
        .expect("fs/watch should succeed through embedded gateway");
    assert_eq!(
        watch.path.as_ref(),
        codex_home
            .path()
            .join("config.toml")
            .canonicalize()
            .expect("watched path should canonicalize")
    );

    let watched_config_path = codex_home.path().join("config.toml");
    let mut watched_config = std::fs::read_to_string(&watched_config_path)
        .expect("watched config should remain readable");
    watched_config.push_str("\n# fs watch notification fixture\n");
    std::fs::write(&watched_config_path, watched_config).expect("watched config should be updated");

    let fs_changed = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
                && notification.watch_id == "embedded-watch"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed notification should arrive after embedded fs/watch");
    assert_eq!(fs_changed.watch_id, "embedded-watch");
    let watched_config_path = watched_config_path
        .canonicalize()
        .expect("watched config path should canonicalize after update");
    assert!(
        fs_changed
            .changed_paths
            .iter()
            .any(|path| path.as_ref() == watched_config_path.as_path()),
        "fs/changed should include the watched config path: {fs_changed:?}"
    );

    let unwatch: FsUnwatchResponse = client
        .request_typed(ClientRequest::FsUnwatch {
            request_id: RequestId::Integer(18),
            params: FsUnwatchParams {
                watch_id: "embedded-watch".to_string(),
            },
        })
        .await
        .expect("fs/unwatch should succeed through embedded gateway");
    assert_eq!(unwatch, FsUnwatchResponse {});

    let reset: MemoryResetResponse = client
        .request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(19),
            params: None,
        })
        .await
        .expect("memory/reset should succeed through embedded gateway");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = client
        .request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(20),
            params: None,
        })
        .await
        .expect("account/logout should succeed through embedded gateway");
    assert_eq!(logout, LogoutAccountResponse {});

    let feedback: FeedbackUploadResponse = client
        .request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(21),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("embedded gateway parity regression".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        })
        .await
        .expect("feedback/upload should succeed through embedded gateway");
    assert!(!feedback.thread_id.is_empty());

    let legacy_auth: GetAuthStatusResponse = client
        .request_typed(ClientRequest::GetAuthStatus {
            request_id: RequestId::Integer(45),
            params: GetAuthStatusParams {
                include_token: Some(true),
                refresh_token: Some(false),
            },
        })
        .await
        .expect("getAuthStatus should succeed through embedded gateway");
    assert_eq!(
        legacy_auth,
        GetAuthStatusResponse {
            auth_method: None,
            auth_token: None,
            requires_openai_auth: Some(true),
        }
    );

    let diff: GitDiffToRemoteResponse = client
        .request_typed(ClientRequest::GitDiffToRemote {
            request_id: RequestId::Integer(46),
            params: GitDiffToRemoteParams {
                cwd: git_repo.clone(),
            },
        })
        .await
        .expect("gitDiffToRemote should succeed through embedded gateway");
    assert_eq!(diff.sha.0, remote_sha);
    assert!(diff.diff.contains("tracked.txt"));
    assert!(diff.diff.contains("changed"));

    let request_handle = client.request_handle();
    let command_exec_task = tokio::spawn(async move {
        request_handle
            .request_typed::<CommandExecResponse>(ClientRequest::OneOffCommandExec {
                request_id: RequestId::Integer(22),
                params: CommandExecParams {
                    command: vec![
                        "sh".to_string(),
                        "-lc".to_string(),
                        "printf embedded-command".to_string(),
                    ],
                    process_id: Some("proc-embedded".to_string()),
                    tty: false,
                    stream_stdin: false,
                    stream_stdout_stderr: true,
                    output_bytes_cap: None,
                    disable_output_cap: false,
                    disable_timeout: false,
                    timeout_ms: None,
                    cwd: None,
                    env: None,
                    size: None,
                    sandbox_policy: Some(SandboxPolicy::DangerFullAccess.into()),
                    permission_profile: None,
                },
            })
            .await
    });

    let command_output = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta notification should arrive through embedded gateway");
    assert_eq!(command_output.process_id, "proc-embedded");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "ZW1iZWRkZWQtY29tbWFuZA==");
    assert_eq!(command_output.cap_reached, false);

    let command_exec = timeout(Duration::from_secs(5), command_exec_task)
        .await
        .expect("command/exec should finish in time")
        .expect("command/exec task should join")
        .expect("command/exec should succeed through embedded gateway");
    assert_eq!(
        command_exec,
        CommandExecResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    );

    let request_handle = client.request_handle();
    let controlled_command_task = tokio::spawn(async move {
        request_handle
            .request_typed::<CommandExecResponse>(ClientRequest::OneOffCommandExec {
                request_id: RequestId::Integer(41),
                params: CommandExecParams {
                    command: vec![
                        "sh".to_string(),
                        "-lc".to_string(),
                        "while true; do sleep 1; done".to_string(),
                    ],
                    process_id: Some("proc-embedded-control".to_string()),
                    tty: true,
                    stream_stdin: false,
                    stream_stdout_stderr: false,
                    output_bytes_cap: None,
                    disable_output_cap: false,
                    disable_timeout: false,
                    timeout_ms: None,
                    cwd: None,
                    env: None,
                    size: Some(CommandExecTerminalSize { rows: 24, cols: 80 }),
                    sandbox_policy: Some(SandboxPolicy::DangerFullAccess.into()),
                    permission_profile: None,
                },
            })
            .await
    });

    let command_write: CommandExecWriteResponse = timeout(Duration::from_secs(5), async {
        loop {
            match client
                .request_typed(ClientRequest::CommandExecWrite {
                    request_id: RequestId::Integer(42),
                    params: CommandExecWriteParams {
                        process_id: "proc-embedded-control".to_string(),
                        delta_base64: Some("Z28K".to_string()),
                        close_stdin: false,
                    },
                })
                .await
            {
                Ok(response) => break response,
                Err(TypedRequestError::Server { source, .. })
                    if source.message.contains("no active command/exec") =>
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(err) => {
                    panic!("command/exec/write should succeed through embedded gateway: {err}")
                }
            }
        }
    })
    .await
    .expect("command/exec/write should eventually find the active embedded command");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = client
        .request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(43),
            params: CommandExecResizeParams {
                process_id: "proc-embedded-control".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        })
        .await
        .expect("command/exec/resize should succeed through embedded gateway");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = client
        .request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(44),
            params: CommandExecTerminateParams {
                process_id: "proc-embedded-control".to_string(),
            },
        })
        .await
        .expect("command/exec/terminate should succeed through embedded gateway");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    timeout(Duration::from_secs(5), controlled_command_task)
        .await
        .expect("controlled command/exec should finish after terminate")
        .expect("controlled command/exec task should join")
        .expect("controlled command/exec should return a final response");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[path = "embedded_tests_core_late.rs"]
mod embedded_tests_core_late;
