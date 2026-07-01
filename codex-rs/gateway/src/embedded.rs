use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayV2TransportConfig;
use crate::config::GatewayConfig;
use crate::config::GatewayRuntimeMode;
use crate::northbound::http::router_with_observability;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::observability::GatewayObservability;
use crate::observability::init_tracing;
use crate::runtime::AppServerGatewayRuntime;
use crate::runtime::GatewayRuntime;
use crate::runtime::GatewayRuntimeHealthConfig;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use crate::v2::gateway_initialize_response;
use axum::serve;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::EnvironmentManager;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessClientStartArgs;
use codex_arg0::Arg0DispatchPaths;
use codex_config::CloudConfigBundleLoader;
use codex_core::config::Config;
use codex_core::config::LoaderOverrides;
use codex_exec_server::ExecServerRuntimePaths;
use codex_feedback::CodexFeedback;
use codex_otel::OtelProvider;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use toml::Value as TomlValue;

#[path = "embedded_remote.rs"]
mod embedded_remote;

const REMOTE_WORKER_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const UNSUPPORTED_SERVER_REQUEST_CODE: i64 = -32000;
const UNSUPPORTED_SERVER_REQUEST_MESSAGE: &str =
    "gateway HTTP clients cannot respond to app-server server requests yet";

use embedded_remote::RemoteAccountLabelSummary;
use embedded_remote::remote_account_label_summary;
use embedded_remote::run_event_loop;
use embedded_remote::start_remote_runtime_gateway_http_server;

fn gateway_v2_transport_config(gateway_config: &GatewayConfig) -> GatewayV2TransportConfig {
    GatewayV2TransportConfig {
        initialize_timeout_seconds: gateway_config.v2_initialize_timeout.as_secs(),
        client_send_timeout_seconds: gateway_config.v2_client_send_timeout.as_secs(),
        reconnect_retry_backoff_seconds: gateway_config.v2_reconnect_retry_backoff.as_secs(),
        max_pending_server_requests: gateway_config.v2_max_pending_server_requests,
        max_pending_client_requests: gateway_config.v2_max_pending_client_requests,
    }
}

pub struct GatewayServer {
    local_addr: SocketAddr,
    _otel: Option<OtelProvider>,
    http_shutdown_tx: Option<oneshot::Sender<()>>,
    event_shutdown_txs: Vec<oneshot::Sender<()>>,
    serve_task: JoinHandle<io::Result<()>>,
    event_tasks: Vec<JoinHandle<io::Result<()>>>,
}

impl GatewayServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(mut self) -> io::Result<()> {
        if let Some(http_shutdown_tx) = self.http_shutdown_tx.take() {
            let _ = http_shutdown_tx.send(());
        }
        for event_shutdown_tx in self.event_shutdown_txs.drain(..) {
            let _ = event_shutdown_tx.send(());
        }

        let serve_result = self
            .serve_task
            .await
            .map_err(|err| io::Error::other(format!("gateway serve task failed: {err}")))?;

        for event_task in self.event_tasks {
            let event_result = event_task
                .await
                .map_err(|err| io::Error::other(format!("gateway event task failed: {err}")))?;
            event_result?;
        }
        serve_result
    }
}

pub async fn start_gateway_server(
    gateway_config: GatewayConfig,
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_overrides: Vec<(String, TomlValue)>,
    loader_overrides: LoaderOverrides,
) -> io::Result<GatewayServer> {
    validate_gateway_config(&gateway_config)?;
    let otel = codex_core::otel_init::build_provider(
        &config,
        env!("CARGO_PKG_VERSION"),
        Some("codex-gateway"),
        /*default_analytics_enabled*/ false,
    )
    .map_err(|err| io::Error::other(format!("error loading otel config: {err}")))?;
    init_tracing(otel.as_ref());

    match &gateway_config.runtime_mode {
        GatewayRuntimeMode::Embedded => {
            let environment_manager =
                gateway_environment_manager(&gateway_config, &arg0_paths).await?;
            validate_gateway_environment_manager(&environment_manager).await?;
            tracing::info!(
                runtime_mode = "embedded",
                execution_mode = match gateway_execution_mode(&gateway_config) {
                    GatewayExecutionMode::InProcess => "in_process",
                    GatewayExecutionMode::ExecServer => "exec_server",
                    GatewayExecutionMode::WorkerManaged => "worker_managed",
                },
                v2_compatibility = "embedded",
                v2_initialize_timeout_seconds = gateway_config.v2_initialize_timeout.as_secs(),
                v2_client_send_timeout_seconds = gateway_config.v2_client_send_timeout.as_secs(),
                v2_reconnect_retry_backoff_seconds =
                    gateway_config.v2_reconnect_retry_backoff.as_secs(),
                v2_max_pending_server_requests = gateway_config.v2_max_pending_server_requests,
                v2_max_pending_client_requests = gateway_config.v2_max_pending_client_requests,
                exec_server_url = gateway_config.exec_server_url.as_deref(),
                "starting gateway with embedded app-server runtime"
            );
            let config_warnings: Vec<codex_app_server_protocol::ConfigWarningNotification> = config
                .startup_warnings
                .iter()
                .map(
                    |warning| codex_app_server_protocol::ConfigWarningNotification {
                        summary: warning.clone(),
                        details: None,
                        path: None,
                        range: None,
                    },
                )
                .collect();
            let state_db = codex_core::init_state_db(&config).await;
            let initialize_response = gateway_initialize_response(&config);
            let start_args = InProcessClientStartArgs {
                arg0_paths: arg0_paths.clone(),
                config: Arc::new(config.clone()),
                cli_overrides: cli_overrides.clone(),
                loader_overrides: loader_overrides.clone(),
                strict_config: false,
                cloud_config_bundle: CloudConfigBundleLoader::default(),
                feedback: CodexFeedback::new(),
                log_db: None,
                state_db,
                environment_manager: environment_manager.clone(),
                config_warnings: config_warnings.clone(),
                session_source: gateway_config.session_source.clone(),
                enable_codex_api_key_env: gateway_config.enable_codex_api_key_env,
                client_name: gateway_config.client_name.clone(),
                client_version: gateway_config.client_version.clone(),
                experimental_api: gateway_config.experimental_api,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: gateway_config.channel_capacity,
            };
            let app_server = AppServerClient::InProcess(
                InProcessAppServerClient::start(start_args.clone()).await?,
            );

            start_single_runtime_gateway_http_server(
                gateway_config,
                otel,
                app_server,
                Some(Arc::new(GatewayV2SessionFactory::embedded(
                    start_args,
                    initialize_response,
                ))),
            )
            .await
        }
        GatewayRuntimeMode::Remote => {
            if gateway_config.exec_server_url.is_some() {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    "remote gateway runtime does not support `--exec-server-url`; configure execution on the remote app-server workers instead",
                ));
            }
            let account_label_summary = gateway_config
                .remote_runtime
                .as_ref()
                .map(remote_account_label_summary);
            tracing::info!(
                runtime_mode = "remote",
                execution_mode = "worker_managed",
                v2_compatibility = gateway_config.remote_runtime.as_ref().map_or(
                    "remote_multi_worker",
                    |remote_runtime| {
                        if remote_runtime.workers.len() == 1 {
                            "remote_single_worker"
                        } else {
                            "remote_multi_worker"
                        }
                    }
                ),
                v2_initialize_timeout_seconds = gateway_config.v2_initialize_timeout.as_secs(),
                v2_client_send_timeout_seconds = gateway_config.v2_client_send_timeout.as_secs(),
                v2_reconnect_retry_backoff_seconds =
                    gateway_config.v2_reconnect_retry_backoff.as_secs(),
                v2_max_pending_server_requests = gateway_config.v2_max_pending_server_requests,
                v2_max_pending_client_requests = gateway_config.v2_max_pending_client_requests,
                remote_worker_count = gateway_config
                    .remote_runtime
                    .as_ref()
                    .map_or(0, |remote_runtime| remote_runtime.workers.len()),
                remote_account_labels_complete = account_label_summary
                    .as_ref()
                    .is_some_and(|summary| summary.complete),
                remote_unlabeled_account_worker_count = account_label_summary
                    .as_ref()
                    .map_or(0, RemoteAccountLabelSummary::unlabeled_worker_count),
                "starting gateway with remote app-server workers"
            );
            start_remote_runtime_gateway_http_server(
                gateway_config,
                otel,
                gateway_initialize_response(&config),
            )
            .await
        }
    }
}

pub async fn start_embedded_gateway_server(
    mut gateway_config: GatewayConfig,
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_overrides: Vec<(String, TomlValue)>,
    loader_overrides: LoaderOverrides,
) -> io::Result<GatewayServer> {
    gateway_config.runtime_mode = GatewayRuntimeMode::Embedded;
    start_gateway_server(
        gateway_config,
        arg0_paths,
        config,
        cli_overrides,
        loader_overrides,
    )
    .await
}

fn validate_gateway_config(gateway_config: &GatewayConfig) -> io::Result<()> {
    if gateway_config.v2_initialize_timeout.is_zero() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 initialize timeout must be greater than zero",
        ));
    }

    if gateway_config.v2_client_send_timeout.is_zero() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 client send timeout must be greater than zero",
        ));
    }

    if gateway_config.v2_reconnect_retry_backoff.is_zero() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 reconnect retry backoff must be greater than zero",
        ));
    }

    if gateway_config.v2_max_pending_server_requests == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 max pending server requests must be greater than zero",
        ));
    }

    if gateway_config.v2_max_pending_client_requests == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 max pending client requests must be greater than zero",
        ));
    }

    if let Some(remote_runtime) = &gateway_config.remote_runtime {
        for (worker_id, worker) in remote_runtime.workers.iter().enumerate() {
            if worker.websocket_url.trim().is_empty() {
                return Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("remote worker {worker_id} websocket URL must not be blank"),
                ));
            }
        }
    }

    Ok(())
}

async fn gateway_environment_manager(
    gateway_config: &GatewayConfig,
    arg0_paths: &Arg0DispatchPaths,
) -> io::Result<Arc<EnvironmentManager>> {
    let local_runtime_paths = arg0_paths
        .codex_self_exe
        .clone()
        .or_else(|| std::env::current_exe().ok())
        .map(|codex_self_exe| {
            ExecServerRuntimePaths::new(codex_self_exe, arg0_paths.codex_linux_sandbox_exe.clone())
        })
        .transpose()?;
    Ok(Arc::new(
        EnvironmentManager::create_for_tests(
            gateway_config.exec_server_url.clone(),
            local_runtime_paths,
        )
        .await,
    ))
}

async fn validate_gateway_environment_manager(
    environment_manager: &Arc<EnvironmentManager>,
) -> io::Result<()> {
    let Some(environment) = environment_manager.default_environment() else {
        return Ok(());
    };

    if environment.exec_server_url().is_none() {
        return Ok(());
    }

    environment.info().await.map(|_| ()).map_err(|err| {
        io::Error::other(format!(
            "failed to initialize remote exec-server environment: {err}"
        ))
    })
}

fn gateway_execution_mode(gateway_config: &GatewayConfig) -> GatewayExecutionMode {
    if gateway_config.exec_server_url.is_some() {
        GatewayExecutionMode::ExecServer
    } else {
        GatewayExecutionMode::InProcess
    }
}

async fn start_single_runtime_gateway_http_server(
    gateway_config: GatewayConfig,
    otel: Option<OtelProvider>,
    app_server: AppServerClient,
    v2_session_factory: Option<Arc<GatewayV2SessionFactory>>,
) -> io::Result<GatewayServer> {
    let (events_tx, _events_rx) =
        tokio::sync::broadcast::channel(gateway_config.event_buffer_capacity);
    let execution_mode = gateway_execution_mode(&gateway_config);
    let request_handle = app_server.request_handle();
    let shared_request_handle = Arc::new(RwLock::new(request_handle.clone()));
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let v2_transport = gateway_v2_transport_config(&gateway_config);
    let observability =
        GatewayObservability::from_otel(otel.as_ref(), gateway_config.audit_logs_enabled)
            .with_operator_events(events_tx.clone());
    let v2_connection_health = observability.v2_connection_health();
    let admission = GatewayAdmissionController::new(GatewayAdmissionConfig {
        request_rate_limit_per_minute: gateway_config.request_rate_limit_per_minute,
        turn_start_quota_per_minute: gateway_config.turn_start_quota_per_minute,
    });
    let runtime: Arc<dyn GatewayRuntime> = Arc::new(AppServerGatewayRuntime::new(
        shared_request_handle.clone(),
        execution_mode,
        events_tx.clone(),
        scope_registry.clone(),
        GatewayRuntimeHealthConfig {
            remote_worker_health: None,
            worker_pool: None,
            v2_transport,
            v2_connection_health: v2_connection_health.clone(),
            observability: observability.clone(),
        },
    ));
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let (event_shutdown_tx, event_shutdown_rx) = oneshot::channel::<()>();
    let http_scope_registry = scope_registry.clone();
    let http_observability = observability.clone();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                http_observability,
                http_scope_registry,
                v2_session_factory,
                GatewayV2Timeouts {
                    initialize: gateway_config.v2_initialize_timeout,
                    client_send: gateway_config.v2_client_send_timeout,
                    reconnect_retry_backoff: gateway_config.v2_reconnect_retry_backoff,
                    max_pending_server_requests: gateway_config.v2_max_pending_server_requests,
                    max_pending_client_requests: gateway_config.v2_max_pending_client_requests,
                },
            ),
        )
        .with_graceful_shutdown(async move {
            let _ = http_shutdown_rx.await;
        })
        .await
        .map_err(io::Error::other)
    });
    let event_task = tokio::spawn(run_event_loop(
        app_server,
        AppServerGatewayRuntime::new(
            shared_request_handle,
            execution_mode,
            events_tx,
            scope_registry,
            GatewayRuntimeHealthConfig {
                remote_worker_health: None,
                worker_pool: None,
                v2_transport,
                v2_connection_health,
                observability,
            },
        ),
        event_shutdown_rx,
    ));

    Ok(GatewayServer {
        local_addr,
        _otel: otel,
        http_shutdown_tx: Some(http_shutdown_tx),
        event_shutdown_txs: vec![event_shutdown_tx],
        serve_task,
        event_tasks: vec![event_task],
    })
}

#[cfg(test)]
#[path = "embedded_tests.rs"]
mod tests;
