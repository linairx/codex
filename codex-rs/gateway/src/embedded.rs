use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayServerRequest;
use crate::api::GatewayV2TransportConfig;
use crate::config::GatewayConfig;
use crate::config::GatewayRemoteWorkerConfig;
use crate::config::GatewayRuntimeMode;
use crate::event::GatewayEvent;
use crate::northbound::http::router_with_observability;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::observability::GatewayObservability;
use crate::observability::init_tracing;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_runtime::RemoteWorkerGatewayRuntime;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::AppServerGatewayRuntime;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use crate::v2::gateway_initialize_response;
use axum::serve;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::AppServerEvent;
use codex_app_server_client::EnvironmentManager;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessClientStartArgs;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ServerNotification;
use codex_arg0::Arg0DispatchPaths;
use codex_core::config::Config;
use codex_core::config_loader::CloudRequirementsLoader;
use codex_core::config_loader::LoaderOverrides;
use codex_feedback::CodexFeedback;
use codex_otel::OtelProvider;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::sleep;
use toml::Value as TomlValue;

const REMOTE_WORKER_RECONNECT_DELAY: Duration = Duration::from_millis(250);

fn gateway_v2_transport_config(gateway_config: &GatewayConfig) -> GatewayV2TransportConfig {
    GatewayV2TransportConfig {
        initialize_timeout_seconds: gateway_config.v2_initialize_timeout.as_secs(),
        client_send_timeout_seconds: gateway_config.v2_client_send_timeout.as_secs(),
        max_pending_server_requests: gateway_config.v2_max_pending_server_requests,
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
            let environment_manager = gateway_environment_manager(&gateway_config);
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
                v2_max_pending_server_requests = gateway_config.v2_max_pending_server_requests,
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
            let initialize_response = gateway_initialize_response(&config);
            let start_args = InProcessClientStartArgs {
                arg0_paths: arg0_paths.clone(),
                config: Arc::new(config.clone()),
                cli_overrides: cli_overrides.clone(),
                loader_overrides: loader_overrides.clone(),
                cloud_requirements: CloudRequirementsLoader::default(),
                feedback: CodexFeedback::new(),
                log_db: None,
                environment_manager: environment_manager.clone(),
                config_warnings: config_warnings.clone(),
                session_source: gateway_config.session_source.clone(),
                enable_codex_api_key_env: gateway_config.enable_codex_api_key_env,
                client_name: gateway_config.client_name.clone(),
                client_version: gateway_config.client_version.clone(),
                experimental_api: gateway_config.experimental_api,
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
                v2_max_pending_server_requests = gateway_config.v2_max_pending_server_requests,
                remote_worker_count = gateway_config
                    .remote_runtime
                    .as_ref()
                    .map_or(0, |remote_runtime| remote_runtime.workers.len()),
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

fn remote_connect_args(
    remote_runtime: &GatewayRemoteWorkerConfig,
    gateway_config: &GatewayConfig,
) -> RemoteAppServerConnectArgs {
    RemoteAppServerConnectArgs {
        websocket_url: remote_runtime.websocket_url.clone(),
        auth_token: remote_runtime.auth_token.clone(),
        client_name: gateway_config.client_name.clone(),
        client_version: gateway_config.client_version.clone(),
        experimental_api: gateway_config.experimental_api,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: gateway_config.channel_capacity,
    }
}

fn missing_remote_runtime_config() -> io::Error {
    io::Error::new(
        ErrorKind::InvalidInput,
        "remote gateway runtime requires remote websocket configuration",
    )
}

fn validate_gateway_config(gateway_config: &GatewayConfig) -> io::Result<()> {
    if gateway_config.v2_max_pending_server_requests == 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "gateway v2 max pending server requests must be greater than zero",
        ));
    }

    Ok(())
}

fn gateway_environment_manager(gateway_config: &GatewayConfig) -> Arc<EnvironmentManager> {
    Arc::new(EnvironmentManager::new(
        gateway_config.exec_server_url.clone(),
    ))
}

async fn validate_gateway_environment_manager(
    environment_manager: &Arc<EnvironmentManager>,
) -> io::Result<()> {
    if !environment_manager.is_remote() {
        return Ok(());
    }

    environment_manager
        .current()
        .await
        .map(|_| ())
        .map_err(|err| {
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
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let v2_transport = gateway_v2_transport_config(&gateway_config);
    let observability =
        GatewayObservability::from_otel(otel.as_ref(), gateway_config.audit_logs_enabled);
    let admission = GatewayAdmissionController::new(GatewayAdmissionConfig {
        request_rate_limit_per_minute: gateway_config.request_rate_limit_per_minute,
        turn_start_quota_per_minute: gateway_config.turn_start_quota_per_minute,
    });
    let runtime: Arc<dyn GatewayRuntime> = Arc::new(AppServerGatewayRuntime::new(
        request_handle.clone(),
        execution_mode,
        events_tx.clone(),
        scope_registry.clone(),
        v2_transport,
    ));
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let (event_shutdown_tx, event_shutdown_rx) = oneshot::channel::<()>();
    let http_scope_registry = scope_registry.clone();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                observability,
                http_scope_registry,
                v2_session_factory,
                GatewayV2Timeouts {
                    initialize: gateway_config.v2_initialize_timeout,
                    client_send: gateway_config.v2_client_send_timeout,
                    max_pending_server_requests: gateway_config.v2_max_pending_server_requests,
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
            request_handle,
            execution_mode,
            events_tx,
            scope_registry,
            v2_transport,
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

async fn start_remote_runtime_gateway_http_server(
    gateway_config: GatewayConfig,
    otel: Option<OtelProvider>,
    initialize_response: codex_app_server_protocol::InitializeResponse,
) -> io::Result<GatewayServer> {
    let remote_runtime = gateway_config
        .remote_runtime
        .as_ref()
        .ok_or_else(missing_remote_runtime_config)?;
    if remote_runtime.workers.is_empty() {
        return Err(missing_remote_runtime_config());
    }

    let (events_tx, _events_rx) =
        tokio::sync::broadcast::channel(gateway_config.event_buffer_capacity);
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let v2_transport = gateway_v2_transport_config(&gateway_config);
    let observability =
        GatewayObservability::from_otel(otel.as_ref(), gateway_config.audit_logs_enabled);
    let admission = GatewayAdmissionController::new(GatewayAdmissionConfig {
        request_rate_limit_per_minute: gateway_config.request_rate_limit_per_minute,
        turn_start_quota_per_minute: gateway_config.turn_start_quota_per_minute,
    });
    let mut workers = Vec::with_capacity(remote_runtime.workers.len());
    let mut app_servers = Vec::with_capacity(remote_runtime.workers.len());
    for (worker_id, worker) in remote_runtime.workers.iter().enumerate() {
        let (managed_worker, app_server) =
            GatewayRemoteWorker::connect(worker_id, remote_connect_args(worker, &gateway_config))
                .await?;
        workers.push(managed_worker);
        app_servers.push(app_server);
    }
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new(
        remote_runtime
            .workers
            .iter()
            .map(|worker| worker.websocket_url.clone())
            .collect(),
    ));
    let v2_session_factory = if remote_runtime.workers.len() == 1 {
        Some(Arc::new(GatewayV2SessionFactory::remote_single(
            remote_connect_args(&remote_runtime.workers[0], &gateway_config),
            initialize_response,
        )))
    } else {
        Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            remote_runtime
                .workers
                .iter()
                .map(|worker| remote_connect_args(worker, &gateway_config))
                .collect(),
            initialize_response,
        )))
    };
    let runtime: Arc<dyn GatewayRuntime> = Arc::new(
        RemoteWorkerGatewayRuntime::new(
            workers.clone(),
            remote_runtime.selection_policy,
            events_tx.clone(),
            scope_registry.clone(),
            worker_health.clone(),
            v2_transport,
        )
        .map_err(|err| io::Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?,
    );
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let http_scope_registry = scope_registry.clone();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                observability,
                http_scope_registry,
                v2_session_factory,
                GatewayV2Timeouts {
                    initialize: gateway_config.v2_initialize_timeout,
                    client_send: gateway_config.v2_client_send_timeout,
                    max_pending_server_requests: gateway_config.v2_max_pending_server_requests,
                },
            ),
        )
        .with_graceful_shutdown(async move {
            let _ = http_shutdown_rx.await;
        })
        .await
        .map_err(io::Error::other)
    });

    let mut event_shutdown_txs = Vec::with_capacity(app_servers.len());
    let mut event_tasks = Vec::with_capacity(app_servers.len());
    for (worker, app_server) in workers.into_iter().zip(app_servers.into_iter()) {
        let runtime = AppServerGatewayRuntime::new_with_worker_id(
            worker.request_handle(),
            Some(worker.id()),
            GatewayExecutionMode::WorkerManaged,
            events_tx.clone(),
            scope_registry.clone(),
            Some(worker_health.clone()),
            v2_transport,
        );
        let (event_shutdown_tx, event_shutdown_rx) = oneshot::channel::<()>();
        event_shutdown_txs.push(event_shutdown_tx);
        event_tasks.push(tokio::spawn(run_remote_worker_loop(
            worker,
            app_server,
            runtime,
            event_shutdown_rx,
        )));
    }

    Ok(GatewayServer {
        local_addr,
        _otel: otel,
        http_shutdown_tx: Some(http_shutdown_tx),
        event_shutdown_txs,
        serve_task,
        event_tasks,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteWorkerLoopExit {
    Shutdown,
    Disconnected,
}

async fn run_remote_worker_loop(
    worker: GatewayRemoteWorker,
    app_server: codex_app_server_client::RemoteAppServerClient,
    runtime: AppServerGatewayRuntime,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> io::Result<()> {
    let mut app_server = AppServerClient::Remote(app_server);
    loop {
        match run_remote_worker_session(app_server, &runtime, &mut shutdown_rx).await? {
            RemoteWorkerLoopExit::Shutdown => break,
            RemoteWorkerLoopExit::Disconnected => {
                let mut reconnected_client = None;
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        _ = sleep(REMOTE_WORKER_RECONNECT_DELAY) => {
                            match worker.reconnect().await {
                                Ok(client) => {
                                    runtime.mark_worker_healthy();
                                    runtime.publish_event(GatewayEvent::reconnected(
                                        worker.id(),
                                        worker.websocket_url().to_string(),
                                    ));
                                    reconnected_client = Some(AppServerClient::Remote(client));
                                    break;
                                }
                                Err(err) => {
                                    runtime.mark_worker_unhealthy(Some(format!(
                                        "failed to reconnect remote worker {} at `{}`: {err}",
                                        worker.id(),
                                        worker.websocket_url(),
                                    )));
                                }
                            }
                        }
                    }
                }

                let Some(client) = reconnected_client else {
                    break;
                };
                app_server = client;
            }
        }
    }

    runtime.clear_owned_pending_server_requests();
    Ok(())
}

async fn run_remote_worker_session(
    mut app_server: AppServerClient,
    runtime: &AppServerGatewayRuntime,
    shutdown_rx: &mut oneshot::Receiver<()>,
) -> io::Result<RemoteWorkerLoopExit> {
    loop {
        tokio::select! {
            _ = &mut *shutdown_rx => {
                runtime.clear_owned_pending_server_requests();
                app_server.shutdown().await?;
                return Ok(RemoteWorkerLoopExit::Shutdown);
            }
            event = app_server.next_event() => {
                let Some(event) = event else {
                    runtime.mark_worker_unhealthy(Some(
                        "remote app server event stream ended".to_string(),
                    ));
                    runtime.clear_owned_pending_server_requests();
                    app_server.shutdown().await?;
                    return Ok(RemoteWorkerLoopExit::Disconnected);
                };
                let disconnected = matches!(event, AppServerEvent::Disconnected { .. });
                handle_app_server_event(event, &app_server, runtime).await?;
                if disconnected {
                    runtime.clear_owned_pending_server_requests();
                    app_server.shutdown().await?;
                    return Ok(RemoteWorkerLoopExit::Disconnected);
                }
            }
        }
    }
}

const UNSUPPORTED_SERVER_REQUEST_CODE: i64 = -32000;
const UNSUPPORTED_SERVER_REQUEST_MESSAGE: &str =
    "gateway HTTP clients cannot respond to app-server server requests yet";

async fn run_event_loop(
    mut app_server: AppServerClient,
    runtime: AppServerGatewayRuntime,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> io::Result<()> {
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                break;
            }
            event = app_server.next_event() => {
                let Some(event) = event else {
                    break;
                };
                handle_app_server_event(event, &app_server, &runtime).await?;
            }
        }
    }

    runtime.clear_owned_pending_server_requests();
    app_server.shutdown().await
}

async fn handle_app_server_event(
    event: AppServerEvent,
    app_server: &AppServerClient,
    runtime: &AppServerGatewayRuntime,
) -> io::Result<()> {
    match event {
        AppServerEvent::Lagged { skipped } => {
            runtime.publish_event(GatewayEvent::lagged(skipped));
        }
        AppServerEvent::ServerNotification(notification) => {
            if let ServerNotification::ServerRequestResolved(payload) = &notification {
                runtime.clear_pending_server_request(&payload.request_id);
            }
            runtime.publish_event(GatewayEvent::from_notification(notification));
        }
        AppServerEvent::ServerRequest(request) => {
            let request_id = request.id().clone();
            match GatewayServerRequest::try_from(request.clone()) {
                Ok(gateway_request) => {
                    let Some(context) = runtime.thread_context(gateway_request.thread_id()) else {
                        runtime.publish_event(GatewayEvent::rejected_server_request(
                            &request,
                            "gateway scope not found for thread",
                        ));
                        app_server
                            .reject_server_request(
                                request.id().clone(),
                                JSONRPCErrorError {
                                    code: UNSUPPORTED_SERVER_REQUEST_CODE,
                                    message: "gateway scope not found for thread".to_string(),
                                    data: None,
                                },
                            )
                            .await?;
                        return Ok(());
                    };
                    runtime.store_pending_server_request(
                        request_id.clone(),
                        &gateway_request,
                        context,
                    );
                    runtime.publish_event(GatewayEvent::requested_server_request(
                        request_id,
                        gateway_request,
                    ));
                }
                Err(request) => {
                    runtime.publish_event(GatewayEvent::rejected_server_request(
                        &request,
                        UNSUPPORTED_SERVER_REQUEST_MESSAGE,
                    ));
                    app_server
                        .reject_server_request(
                            request.id().clone(),
                            JSONRPCErrorError {
                                code: UNSUPPORTED_SERVER_REQUEST_CODE,
                                message: UNSUPPORTED_SERVER_REQUEST_MESSAGE.to_string(),
                                data: None,
                            },
                        )
                        .await?;
                }
            }
        }
        AppServerEvent::Disconnected { message } => {
            runtime.mark_worker_unhealthy(Some(message.clone()));
            runtime.clear_owned_pending_server_requests();
            runtime.publish_event(GatewayEvent::disconnected(message));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::gateway_environment_manager;
    use super::gateway_execution_mode;
    use super::start_embedded_gateway_server;
    use super::start_gateway_server;
    use crate::api::CreateThreadRequest;
    use crate::api::GatewayExecutionMode;
    use crate::api::GatewayHealthResponse;
    use crate::api::GatewayHealthStatus;
    use crate::api::GatewayV2CompatibilityMode;
    use crate::api::ListThreadsResponse;
    use crate::api::ThreadResponse;
    use crate::config::GatewayConfig;
    use crate::config::GatewayRemoteRuntimeConfig;
    use crate::config::GatewayRemoteSelectionPolicy;
    use crate::config::GatewayRemoteWorkerConfig;
    use crate::config::GatewayRuntimeMode;
    use app_test_support::ChatGptAuthFixture;
    use app_test_support::create_apply_patch_sse_response;
    use app_test_support::create_exec_command_sse_response;
    use app_test_support::write_chatgpt_auth;
    use axum::Json;
    use axum::Router;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::http::StatusCode;
    use axum::http::Uri;
    use axum::http::header::AUTHORIZATION;
    use axum::routing::get;
    use codex_app_server_client::AppServerEvent;
    use codex_app_server_client::RemoteAppServerClient;
    use codex_app_server_client::RemoteAppServerConnectArgs;
    use codex_app_server_client::TypedRequestError;
    use codex_app_server_protocol::AppsListParams;
    use codex_app_server_protocol::AppsListResponse;
    use codex_app_server_protocol::ChatgptAuthTokensRefreshResponse;
    use codex_app_server_protocol::ClientRequest;
    use codex_app_server_protocol::CommandExecutionApprovalDecision;
    use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
    use codex_app_server_protocol::ConfigBatchWriteParams;
    use codex_app_server_protocol::ConfigValueWriteParams;
    use codex_app_server_protocol::ConfigWriteResponse;
    use codex_app_server_protocol::ExternalAgentConfigDetectParams;
    use codex_app_server_protocol::ExternalAgentConfigDetectResponse;
    use codex_app_server_protocol::ExternalAgentConfigImportParams;
    use codex_app_server_protocol::ExternalAgentConfigImportResponse;
    use codex_app_server_protocol::FileChangeApprovalDecision;
    use codex_app_server_protocol::FileChangeRequestApprovalResponse;
    use codex_app_server_protocol::GetAccountParams;
    use codex_app_server_protocol::GetAccountRateLimitsResponse;
    use codex_app_server_protocol::GetAccountResponse;
    use codex_app_server_protocol::JSONRPCError;
    use codex_app_server_protocol::JSONRPCErrorError;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::ListMcpServerStatusParams;
    use codex_app_server_protocol::ListMcpServerStatusResponse;
    use codex_app_server_protocol::LogoutAccountResponse;
    use codex_app_server_protocol::McpElicitationSchema;
    use codex_app_server_protocol::McpServerElicitationAction;
    use codex_app_server_protocol::McpServerElicitationRequest;
    use codex_app_server_protocol::McpServerElicitationRequestResponse;
    use codex_app_server_protocol::McpServerStatusDetail;
    use codex_app_server_protocol::MemoryResetResponse;
    use codex_app_server_protocol::MergeStrategy;
    use codex_app_server_protocol::ModelListParams;
    use codex_app_server_protocol::ModelListResponse;
    use codex_app_server_protocol::PermissionGrantScope;
    use codex_app_server_protocol::PermissionsRequestApprovalResponse;
    use codex_app_server_protocol::PluginAuthPolicy;
    use codex_app_server_protocol::PluginInstallParams;
    use codex_app_server_protocol::PluginInstallResponse;
    use codex_app_server_protocol::PluginListParams;
    use codex_app_server_protocol::PluginListResponse;
    use codex_app_server_protocol::PluginReadParams;
    use codex_app_server_protocol::PluginReadResponse;
    use codex_app_server_protocol::PluginUninstallParams;
    use codex_app_server_protocol::PluginUninstallResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ServerRequest;
    use codex_app_server_protocol::ServerRequestResolvedNotification;
    use codex_app_server_protocol::SkillsListParams;
    use codex_app_server_protocol::SkillsListResponse;
    use codex_app_server_protocol::ThreadForkParams;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse as AppServerThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedListParams;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadMemoryMode;
    use codex_app_server_protocol::ThreadMemoryModeSetParams;
    use codex_app_server_protocol::ThreadMemoryModeSetResponse;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadReadResponse as AppServerThreadReadResponse;
    use codex_app_server_protocol::ThreadResumeParams;
    use codex_app_server_protocol::ThreadSetNameParams;
    use codex_app_server_protocol::ThreadSetNameResponse;
    use codex_app_server_protocol::ThreadStartParams;
    use codex_app_server_protocol::ThreadStartResponse as AppServerThreadStartResponse;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ToolRequestUserInputAnswer;
    use codex_app_server_protocol::ToolRequestUserInputQuestion;
    use codex_app_server_protocol::ToolRequestUserInputResponse;
    use codex_app_server_protocol::TurnCompletedNotification;
    use codex_app_server_protocol::TurnStartParams;
    use codex_app_server_protocol::TurnStartResponse;
    use codex_app_server_protocol::TurnStartedNotification;
    use codex_app_server_protocol::TurnStatus;
    use codex_app_server_protocol::UserInput;
    use codex_app_server_protocol::WriteStatus;
    use codex_arg0::Arg0DispatchPaths;
    use codex_config::types::AuthCredentialsStoreMode;
    use codex_core::config::Config;
    use codex_core::config_loader::LoaderOverrides;
    use codex_protocol::config_types::CollaborationMode;
    use codex_protocol::config_types::ModeKind;
    use codex_protocol::config_types::Settings;
    use codex_protocol::openai_models::ReasoningEffort;
    use codex_protocol::protocol::SessionSource;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use rmcp::handler::server::ServerHandler;
    use rmcp::model::BooleanSchema;
    use rmcp::model::CallToolRequestParams;
    use rmcp::model::CallToolResult;
    use rmcp::model::Content;
    use rmcp::model::CreateElicitationRequestParams;
    use rmcp::model::ElicitationAction;
    use rmcp::model::ElicitationSchema;
    use rmcp::model::JsonObject;
    use rmcp::model::ListToolsResult;
    use rmcp::model::Meta;
    use rmcp::model::PrimitiveSchema;
    use rmcp::model::ServerCapabilities;
    use rmcp::model::ServerInfo;
    use rmcp::model::Tool;
    use rmcp::model::ToolAnnotations;
    use rmcp::service::RequestContext;
    use rmcp::service::RoleServer;
    use rmcp::transport::StreamableHttpServerConfig;
    use rmcp::transport::StreamableHttpService;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::io::AsyncRead;
    use tokio::io::AsyncWrite;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;
    use tokio::time::Duration;
    use tokio::time::sleep;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
    use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;

    const EMBEDDED_CONNECTOR_ID: &str = "calendar";
    const EMBEDDED_CONNECTOR_NAME: &str = "Calendar";
    const EMBEDDED_TOOL_NAMESPACE: &str = "mcp__codex_apps__calendar";
    const EMBEDDED_CALLABLE_TOOL_NAME: &str = "_confirm_action";
    const EMBEDDED_TOOL_NAME: &str = "calendar_confirm_action";
    const EMBEDDED_TOOL_CALL_ID: &str = "call-calendar-confirm";
    const EMBEDDED_ELICITATION_MESSAGE: &str = "Allow this request?";

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TurnStreamingCoverage {
        saw_thread_active: bool,
        saw_turn_started: bool,
        saw_agent_delta: bool,
        saw_reasoning_summary_delta: bool,
        saw_reasoning_text_delta: bool,
        saw_command_output_delta: bool,
        saw_file_change_delta: bool,
        saw_turn_completed: bool,
    }

    #[test]
    fn embedded_gateway_environment_manager_preserves_remote_exec_server_url() {
        let environment_manager = gateway_environment_manager(&GatewayConfig {
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            ..GatewayConfig::default()
        });

        assert_eq!(environment_manager.is_remote(), true);
        assert_eq!(
            environment_manager.exec_server_url(),
            Some("ws://127.0.0.1:9753")
        );
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
    async fn gateway_rejects_zero_max_pending_v2_server_requests() {
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");

        let result = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                v2_max_pending_server_requests: 0,
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await;

        let err = match result {
            Ok(_server) => panic!("gateway should reject zero pending server request limit"),
            Err(err) => err,
        };

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "gateway v2 max pending server requests must be greater than zero"
        );
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
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
                    persist_extended_history: false,
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

        let thread_started = timeout(Duration::from_secs(5), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(ServerNotification::ThreadStarted(
                    notification,
                )) = event
                    && notification.thread.id == started.thread.id
                {
                    break notification;
                }
            }
        })
        .await
        .expect("thread/started notification should arrive");
        assert_eq!(thread_started.thread, started.thread);

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn embedded_server_supports_drop_in_v2_client_bootstrap_setup_methods() {
        let codex_home = tempdir().expect("tempdir");
        write_embedded_plugin_fixture(codex_home.path());
        std::fs::write(
            codex_home.path().join("config.toml"),
            "model = \"gpt-5\"\n\n[features]\nplugins = true\n",
        )
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

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
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
                },
            })
            .await
            .expect("externalAgentConfig/import should succeed through embedded gateway");
        assert_eq!(imported, ExternalAgentConfigImportResponse {});

        let skills: SkillsListResponse = client
            .request_typed(ClientRequest::SkillsList {
                request_id: RequestId::Integer(3),
                params: SkillsListParams {
                    cwds: vec![codex_home.path().to_path_buf()],
                    force_reload: false,
                    per_cwd_extra_user_roots: None,
                },
            })
            .await
            .expect("skills/list should succeed through embedded gateway");
        assert_eq!(skills.data.len(), 1);
        assert_eq!(skills.data[0].cwd, codex_home.path().to_path_buf());

        let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
            "cwds": [codex_home.path()],
        }))
        .expect("plugin/list params should deserialize");
        let plugins: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(4),
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
                request_id: RequestId::Integer(5),
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

        let plugin_install_params: PluginInstallParams =
            serde_json::from_value(serde_json::json!({
                "marketplacePath": codex_home.path().join(".agents/plugins/marketplace.json"),
                "pluginName": "demo-plugin",
            }))
            .expect("plugin/install params should deserialize");
        let install: PluginInstallResponse = client
            .request_typed(ClientRequest::PluginInstall {
                request_id: RequestId::Integer(6),
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
                request_id: RequestId::Integer(7),
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
                request_id: RequestId::Integer(8),
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
                request_id: RequestId::Integer(9),
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

        let reset: MemoryResetResponse = client
            .request_typed(ClientRequest::MemoryReset {
                request_id: RequestId::Integer(10),
                params: None,
            })
            .await
            .expect("memory/reset should succeed through embedded gateway");
        assert_eq!(reset, MemoryResetResponse {});

        let logout: LogoutAccountResponse = client
            .request_typed(ClientRequest::LogoutAccount {
                request_id: RequestId::Integer(11),
                params: None,
            })
            .await
            .expect("account/logout should succeed through embedded gateway");
        assert_eq!(logout, LogoutAccountResponse {});

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn embedded_server_supports_server_request_roundtrip_over_v2() {
        let codex_home = tempdir().expect("tempdir");
        let model_server = start_mock_responses_server_sequence(vec![
            mock_responses_request_user_input_sse_body("call1"),
            mock_responses_sse_body("done"),
        ])
        .await;
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

        let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: Some("mock-model".to_string()),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let turn_started_response: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "ask something".to_string(),
                        text_elements: Vec::new(),
                    }],
                    responsesapi_client_metadata: None,
                    cwd: None,
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox_policy: None,
                    model: Some("mock-model".to_string()),
                    service_tier: None,
                    effort: Some(ReasoningEffort::Medium),
                    summary: None,
                    personality: None,
                    output_schema: None,
                    collaboration_mode: Some(CollaborationMode {
                        mode: ModeKind::Plan,
                        settings: Settings {
                            model: "mock-model".to_string(),
                            reasoning_effort: Some(ReasoningEffort::Medium),
                            developer_instructions: None,
                        },
                    }),
                },
            })
            .await
            .expect("turn/start should succeed through embedded gateway");
        let turn_id = turn_started_response.turn.id.clone();

        let (request_id, params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("server request should arrive");
        assert_eq!(params.thread_id, started.thread.id);
        assert_eq!(params.turn_id, turn_id);
        assert_eq!(params.item_id, "call1");
        assert_eq!(params.questions.len(), 1);
        let resolved_request_id = request_id.clone();

        let mut answers = HashMap::new();
        answers.insert(
            "confirm_path".to_string(),
            ToolRequestUserInputAnswer {
                answers: vec!["yes".to_string()],
            },
        );
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ToolRequestUserInputResponse { answers })
                    .expect("server request response should serialize"),
            )
            .await
            .expect("server request should resolve");

        let mut saw_resolved = false;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(notification) = event {
                    match notification {
                        ServerNotification::ServerRequestResolved(
                            ServerRequestResolvedNotification {
                                thread_id,
                                request_id,
                            },
                        ) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(request_id, resolved_request_id);
                            saw_resolved = true;
                        }
                        ServerNotification::TurnCompleted(TurnCompletedNotification {
                            thread_id,
                            turn,
                        }) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(turn.id, turn_id);
                            assert_eq!(turn.status, TurnStatus::Completed);
                            assert_eq!(
                                saw_resolved, true,
                                "serverRequest/resolved should arrive first"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
        })
        .await
        .expect("resolved notification and turn completion should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
        model_server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_supports_permissions_server_request_roundtrip_over_v2() {
        let codex_home = tempdir().expect("tempdir");
        let model_server = start_mock_responses_server_sequence(vec![
            format!(
                "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"function_call\",\"id\":\"call1\",\"call_id\":\"call1\",\"name\":\"request_permissions\",\"arguments\":\"{{\\\"reason\\\":\\\"Select a workspace root\\\",\\\"permissions\\\":{{\\\"file_system\\\":{{\\\"write\\\":[\\\".\\\",\\\"../shared\\\"]}}}}}}\"}}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"function_call\",\"id\":\"call1\",\"call_id\":\"call1\",\"name\":\"request_permissions\",\"arguments\":\"{{\\\"reason\\\":\\\"Select a workspace root\\\",\\\"permissions\\\":{{\\\"file_system\\\":{{\\\"write\\\":[\\\".\\\",\\\"../shared\\\"]}}}}}}\"}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
            ),
            mock_responses_sse_body("done"),
        ])
        .await;
        std::fs::write(
            codex_home.path().join("config.toml"),
            format!(
                r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[features]
request_permissions_tool = true
"#,
                model_server.uri
            ),
        )
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

        let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: Some("mock-model".to_string()),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let turn_started_response: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "pick a directory".to_string(),
                        text_elements: Vec::new(),
                    }],
                    responsesapi_client_metadata: None,
                    cwd: None,
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox_policy: None,
                    model: Some("mock-model".to_string()),
                    service_tier: None,
                    effort: None,
                    summary: None,
                    personality: None,
                    output_schema: None,
                    collaboration_mode: None,
                },
            })
            .await
            .expect("turn/start should succeed through embedded gateway");
        let turn_id = turn_started_response.turn.id.clone();

        let (request_id, params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("permissions server request should arrive");
        assert_eq!(params.thread_id, started.thread.id);
        assert_eq!(params.turn_id, turn_id);
        assert_eq!(params.item_id, "call1");
        assert_eq!(params.reason, Some("Select a workspace root".to_string()));
        let requested_writes = params
            .permissions
            .file_system
            .and_then(|file_system| file_system.write)
            .expect("request should include write permissions");
        assert_eq!(requested_writes.len(), 2);
        let resolved_request_id = request_id.clone();

        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(PermissionsRequestApprovalResponse {
                    permissions: codex_app_server_protocol::GrantedPermissionProfile {
                        network: None,
                        file_system: Some(
                            codex_app_server_protocol::AdditionalFileSystemPermissions {
                                read: None,
                                write: Some(vec![requested_writes[0].clone()]),
                            },
                        ),
                    },
                    scope: PermissionGrantScope::Turn,
                })
                .expect("permissions response should serialize"),
            )
            .await
            .expect("permissions request should resolve");

        let mut saw_resolved = false;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(notification) = event {
                    match notification {
                        ServerNotification::ServerRequestResolved(
                            ServerRequestResolvedNotification {
                                thread_id,
                                request_id,
                            },
                        ) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(request_id, resolved_request_id);
                            saw_resolved = true;
                        }
                        ServerNotification::TurnCompleted(TurnCompletedNotification {
                            thread_id,
                            turn,
                        }) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(turn.id, turn_id);
                            assert_eq!(turn.status, TurnStatus::Completed);
                            assert_eq!(
                                saw_resolved, true,
                                "serverRequest/resolved should arrive first"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
        })
        .await
        .expect("resolved notification and turn completion should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
        model_server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_supports_command_and_file_approval_roundtrips_over_v2() {
        let codex_home = tempdir().expect("tempdir");
        let workspace = codex_home.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace should be created");
        let patch = r#"*** Begin Patch
*** Add File: README.md
+new line
*** End Patch
"#;
        let model_server = start_mock_responses_server_sequence(vec![
            create_exec_command_sse_response("exec-call")
                .expect("exec command response should serialize"),
            mock_responses_sse_body("command complete"),
            create_apply_patch_sse_response(patch, "patch-call")
                .expect("apply patch response should serialize"),
            mock_responses_sse_body("patch complete"),
        ])
        .await;
        std::fs::write(
            codex_home.path().join("config.toml"),
            format!(
                r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
                model_server.uri
            ),
        )
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

        let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: Some("mock-model".to_string()),
                    cwd: Some(workspace.display().to_string()),
                    ephemeral: Some(false),
                    ..Default::default()
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let command_turn: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "run a command".to_string(),
                        text_elements: Vec::new(),
                    }],
                    model: Some("mock-model".to_string()),
                    ..Default::default()
                },
            })
            .await
            .expect("command turn/start should succeed through embedded gateway");
        let command_turn_id = command_turn.turn.id.clone();

        let (command_request_id, command_params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(
                    ServerRequest::CommandExecutionRequestApproval { request_id, params },
                ) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("command approval request should arrive");
        assert_eq!(command_params.thread_id, started.thread.id);
        assert_eq!(command_params.turn_id, command_turn_id);
        assert_eq!(command_params.item_id, "exec-call");
        let resolved_command_request_id = command_request_id.clone();

        client
            .resolve_server_request(
                command_request_id,
                serde_json::to_value(CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Accept,
                })
                .expect("command approval response should serialize"),
            )
            .await
            .expect("command approval request should resolve");

        let mut saw_command_resolved = false;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(notification) = event {
                    match notification {
                        ServerNotification::ServerRequestResolved(
                            ServerRequestResolvedNotification {
                                thread_id,
                                request_id,
                            },
                        ) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(request_id, resolved_command_request_id);
                            saw_command_resolved = true;
                        }
                        ServerNotification::TurnCompleted(TurnCompletedNotification {
                            thread_id,
                            turn,
                        }) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(turn.id, command_turn_id);
                            assert_eq!(turn.status, TurnStatus::Completed);
                            assert_eq!(
                                saw_command_resolved, true,
                                "serverRequest/resolved should arrive first"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
        })
        .await
        .expect("command turn should resolve and complete");

        let file_turn: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(3),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "apply patch".to_string(),
                        text_elements: Vec::new(),
                    }],
                    cwd: Some(workspace.clone()),
                    model: Some("mock-model".to_string()),
                    ..Default::default()
                },
            })
            .await
            .expect("file turn/start should succeed through embedded gateway");
        let file_turn_id = file_turn.turn.id.clone();

        let (file_request_id, file_params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("file approval request should arrive");
        assert_eq!(file_params.thread_id, started.thread.id);
        assert_eq!(file_params.turn_id, file_turn_id);
        assert_eq!(file_params.item_id, "patch-call");
        let resolved_file_request_id = file_request_id.clone();

        client
            .resolve_server_request(
                file_request_id,
                serde_json::to_value(FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Accept,
                })
                .expect("file approval response should serialize"),
            )
            .await
            .expect("file approval request should resolve");

        let mut saw_file_resolved = false;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(notification) = event {
                    match notification {
                        ServerNotification::ServerRequestResolved(
                            ServerRequestResolvedNotification {
                                thread_id,
                                request_id,
                            },
                        ) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(request_id, resolved_file_request_id);
                            saw_file_resolved = true;
                        }
                        ServerNotification::TurnCompleted(TurnCompletedNotification {
                            thread_id,
                            turn,
                        }) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(turn.id, file_turn_id);
                            assert_eq!(turn.status, TurnStatus::Completed);
                            assert_eq!(
                                saw_file_resolved, true,
                                "serverRequest/resolved should arrive first"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
        })
        .await
        .expect("file turn should resolve and complete");

        assert_eq!(
            std::fs::read_to_string(workspace.join("README.md"))
                .expect("README should be written after accepting patch"),
            "new line\n"
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
        model_server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_supports_mcp_elicitation_server_request_roundtrip_over_v2() {
        let codex_home = tempdir().expect("tempdir");
        let model_server = start_mock_responses_server_sequence(vec![
            mock_responses_sse_body("Warmup"),
            mock_responses_namespaced_function_call_sse_body(
                EMBEDDED_TOOL_CALL_ID,
                EMBEDDED_TOOL_NAMESPACE,
                EMBEDDED_CALLABLE_TOOL_NAME,
                "{}",
            ),
            mock_responses_sse_body("Done"),
        ])
        .await;
        let (apps_server_url, apps_server_handle) = start_embedded_gateway_apps_server()
            .await
            .expect("apps server");
        write_embedded_mcp_config_toml(codex_home.path(), &model_server.uri, &apps_server_url)
            .expect("config.toml should be written");
        write_chatgpt_auth(
            codex_home.path(),
            ChatGptAuthFixture::new("chatgpt-token")
                .account_id("account-123")
                .chatgpt_user_id("user-123")
                .chatgpt_account_id("account-123"),
            AuthCredentialsStoreMode::File,
        )
        .expect("chatgpt auth should be written");
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: Some("mock-model".to_string()),
                    ..Default::default()
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let _: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "Warm up connectors.".to_string(),
                        text_elements: Vec::new(),
                    }],
                    model: Some("mock-model".to_string()),
                    ..Default::default()
                },
            })
            .await
            .expect("warmup turn/start should succeed through embedded gateway");

        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    completed,
                )) = event
                {
                    assert_eq!(completed.thread_id, started.thread.id);
                    assert_eq!(completed.turn.status, TurnStatus::Completed);
                    break;
                }
            }
        })
        .await
        .expect("warmup completion should arrive");

        let turn_started_response: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(3),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "Use [$calendar](app://calendar) to run the calendar tool."
                            .to_string(),
                        text_elements: Vec::new(),
                    }],
                    model: Some("mock-model".to_string()),
                    ..Default::default()
                },
            })
            .await
            .expect("turn/start should succeed through embedded gateway");
        let turn_id = turn_started_response.turn.id.clone();

        let (request_id, params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("mcp elicitation request should arrive");
        let requested_schema: McpElicitationSchema = serde_json::from_value(
            serde_json::to_value(
                ElicitationSchema::builder()
                    .required_property("confirmed", PrimitiveSchema::Boolean(BooleanSchema::new()))
                    .build()
                    .expect("schema should build"),
            )
            .expect("schema should serialize"),
        )
        .expect("schema should decode");
        assert_eq!(params.thread_id, started.thread.id);
        assert_eq!(params.turn_id, Some(turn_id.clone()));
        assert_eq!(params.server_name, "codex_apps");
        assert_eq!(
            params.request,
            McpServerElicitationRequest::Form {
                meta: None,
                message: EMBEDDED_ELICITATION_MESSAGE.to_string(),
                requested_schema,
            }
        );
        let resolved_request_id = request_id.clone();

        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Accept,
                    content: Some(serde_json::json!({
                        "confirmed": true,
                    })),
                    meta: None,
                })
                .expect("elicitation response should serialize"),
            )
            .await
            .expect("mcp elicitation request should resolve");

        let mut saw_resolved = false;
        timeout(Duration::from_secs(10), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(notification) = event {
                    match notification {
                        ServerNotification::ServerRequestResolved(
                            ServerRequestResolvedNotification {
                                thread_id,
                                request_id,
                            },
                        ) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(request_id, resolved_request_id);
                            saw_resolved = true;
                        }
                        ServerNotification::TurnCompleted(TurnCompletedNotification {
                            thread_id,
                            turn,
                        }) => {
                            assert_eq!(thread_id, started.thread.id);
                            assert_eq!(turn.id, turn_id);
                            assert_eq!(turn.status, TurnStatus::Completed);
                            assert_eq!(
                                saw_resolved, true,
                                "serverRequest/resolved should arrive first"
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
        })
        .await
        .expect("resolved notification and turn completion should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
        model_server.shutdown().await;
        apps_server_handle.abort();
        let _ = apps_server_handle.await;
    }

    #[tokio::test]
    async fn embedded_server_supports_drop_in_v2_client_turn_workflow() {
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

        let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let turn_started_response: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "hello from embedded gateway".to_string(),
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
                },
            })
            .await
            .expect("turn/start should succeed through embedded gateway");
        assert_eq!(turn_started_response.turn.status, TurnStatus::InProgress);

        let turn_id = turn_started_response.turn.id.clone();
        let mut saw_thread_active = false;
        let mut saw_turn_started = false;
        let mut saw_agent_delta = false;
        let mut saw_turn_completed = false;

        timeout(Duration::from_secs(10), async {
            while !(saw_thread_active && saw_turn_started && saw_agent_delta && saw_turn_completed)
            {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerNotification(
                        ServerNotification::ThreadStatusChanged(notification),
                    ) => {
                        if notification.thread_id == started.thread.id
                            && matches!(notification.status, ThreadStatus::Active { .. })
                        {
                            saw_thread_active = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                        TurnStartedNotification { thread_id, turn },
                    )) => {
                        if thread_id == started.thread.id && turn.id == turn_id {
                            saw_turn_started = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                        notification,
                    )) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == turn_id
                            && notification.delta == "Done"
                        {
                            saw_agent_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                        TurnCompletedNotification { thread_id, turn },
                    )) => {
                        if thread_id == started.thread.id
                            && turn.id == turn_id
                            && turn.status == TurnStatus::Completed
                        {
                            saw_turn_completed = true;
                        }
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("turn lifecycle notifications should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
        model_server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_preserves_unmaterialized_thread_resume_and_fork_errors_over_v2() {
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

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to embedded gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through embedded gateway");

        let resume_error = client
            .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
                request_id: RequestId::Integer(2),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect_err("unmaterialized thread/resume should fail through embedded gateway");
        assert_eq!(
            resume_error.to_string(),
            format!(
                "thread/resume failed: no rollout found for thread id {}",
                started.thread.id
            )
        );
        let TypedRequestError::Server {
            source: resume_source,
            ..
        } = resume_error
        else {
            panic!("thread/resume should return a server JSON-RPC error");
        };
        assert_eq!(
            resume_source.message,
            format!("no rollout found for thread id {}", started.thread.id)
        );

        let fork_error = client
            .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
                request_id: RequestId::Integer(3),
                params: ThreadForkParams {
                    thread_id: started.thread.id.clone(),
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
                    ephemeral: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect_err("unmaterialized thread/fork should fail through embedded gateway");
        assert_eq!(
            fork_error.to_string(),
            format!(
                "thread/fork failed: no rollout found for thread id {}",
                started.thread.id
            )
        );
        let TypedRequestError::Server {
            source: fork_source,
            ..
        } = fork_error
        else {
            panic!("thread/fork should return a server JSON-RPC error");
        };
        assert_eq!(
            fork_source.message,
            format!("no rollout found for thread id {}", started.thread.id)
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_server_forwards_thread_creation_requests() {
        let websocket_url = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-remote",
            "/tmp/project",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: Some("secret-token".to_string()),
                    }],
                }),
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
                cwd: Some("/tmp/project".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("http response");

        let status = response.status();
        let body_text = response.text().await.expect("response text");
        assert_eq!(status, reqwest::StatusCode::OK, "{body_text}");
        let body: ThreadResponse = serde_json::from_str(&body_text).expect("thread response");
        assert_eq!(body.thread.id, "thread-remote");
        assert_eq!(body.thread.preview, "/tmp/project");

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_supports_drop_in_v2_client_bootstrap_and_thread_workflow() {
        let websocket_url = start_mock_remote_workflow_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: None,
                    }],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to remote gateway");

        let account: GetAccountResponse = client
            .request_typed(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .expect("account/read should succeed through remote gateway");
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
            .expect("model/list should succeed through remote gateway");
        assert_eq!(models.data.is_empty(), false);

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(3),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/remote-project".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through remote gateway");
        assert_eq!(started.thread.id, "thread-remote-workflow");

        let listed: AppServerThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(4),
                params: ThreadListParams {
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
            .expect("thread/list should succeed through remote gateway");
        assert_eq!(listed.data.len(), 1);
        assert_eq!(listed.data[0].id, started.thread.id);

        let loaded: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(5),
                params: ThreadLoadedListParams {
                    cursor: None,
                    limit: Some(10),
                },
            })
            .await
            .expect("thread/loaded/list should succeed through remote gateway");
        assert_eq!(loaded.data, vec![started.thread.id.clone()]);

        let read: AppServerThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(6),
                params: ThreadReadParams {
                    thread_id: started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read should succeed through remote gateway");
        assert_eq!(read.thread.id, started.thread.id);
        assert_eq!(read.thread.name, None);

        let renamed_thread_name = "Remote Gateway Thread".to_string();
        let rename_response: ThreadSetNameResponse = client
            .request_typed(ClientRequest::ThreadSetName {
                request_id: RequestId::Integer(7),
                params: ThreadSetNameParams {
                    thread_id: started.thread.id.clone(),
                    name: renamed_thread_name.clone(),
                },
            })
            .await
            .expect("thread/name/set should succeed through remote gateway");
        assert_eq!(rename_response, ThreadSetNameResponse {});

        let renamed: AppServerThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(8),
                params: ThreadReadParams {
                    thread_id: started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read after rename should succeed through remote gateway");
        assert_eq!(renamed.thread.name, Some(renamed_thread_name));

        let memory_mode_response: ThreadMemoryModeSetResponse = client
            .request_typed(ClientRequest::ThreadMemoryModeSet {
                request_id: RequestId::Integer(9),
                params: ThreadMemoryModeSetParams {
                    thread_id: started.thread.id.clone(),
                    mode: ThreadMemoryMode::Enabled,
                },
            })
            .await
            .expect("thread/memoryMode/set should succeed through remote gateway");
        assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

        let thread_started = timeout(Duration::from_secs(5), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerNotification(ServerNotification::ThreadStarted(
                    notification,
                )) = event
                    && notification.thread.id == started.thread.id
                {
                    break notification;
                }
            }
        })
        .await
        .expect("thread/started notification should arrive");
        assert_eq!(thread_started.thread.id, started.thread.id);

        let turn_started_response: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(10),
                params: TurnStartParams {
                    thread_id: started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "hello from remote gateway".to_string(),
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
                },
            })
            .await
            .expect("turn/start should succeed through remote gateway");
        assert_eq!(turn_started_response.turn.id, "turn-remote-workflow");
        assert_eq!(turn_started_response.turn.status, TurnStatus::InProgress);

        let mut coverage = TurnStreamingCoverage::default();
        timeout(Duration::from_secs(5), async {
            while coverage
                != (TurnStreamingCoverage {
                    saw_thread_active: true,
                    saw_turn_started: true,
                    saw_agent_delta: true,
                    saw_reasoning_summary_delta: true,
                    saw_reasoning_text_delta: true,
                    saw_command_output_delta: true,
                    saw_file_change_delta: true,
                    saw_turn_completed: true,
                })
            {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerNotification(
                        ServerNotification::ThreadStatusChanged(notification),
                    ) => {
                        if notification.thread_id == started.thread.id
                            && matches!(
                                notification.status,
                                ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                            )
                        {
                            coverage.saw_thread_active = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                        notification,
                    )) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn.id == "turn-remote-workflow"
                        {
                            coverage.saw_turn_started = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                        notification,
                    )) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == "turn-remote-workflow"
                            && notification.delta == "hello back"
                        {
                            coverage.saw_agent_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ReasoningSummaryTextDelta(notification),
                    ) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == "turn-remote-workflow"
                            && notification.delta == "remote summary"
                            && notification.summary_index == 0
                        {
                            coverage.saw_reasoning_summary_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                        notification,
                    )) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == "turn-remote-workflow"
                            && notification.delta == "remote reasoning"
                            && notification.content_index == 0
                        {
                            coverage.saw_reasoning_text_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::CommandExecutionOutputDelta(notification),
                    ) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == "turn-remote-workflow"
                            && notification.delta == "remote stdout"
                        {
                            coverage.saw_command_output_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::FileChangeOutputDelta(notification),
                    ) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn_id == "turn-remote-workflow"
                            && notification.delta == "remote patch"
                        {
                            coverage.saw_file_change_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                        notification,
                    )) => {
                        if notification.thread_id == started.thread.id
                            && notification.turn.id == "turn-remote-workflow"
                            && notification.turn.status == TurnStatus::Completed
                        {
                            coverage.saw_turn_completed = true;
                        }
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("turn lifecycle notifications should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_supports_drop_in_v2_client_bootstrap_setup_methods() {
        let websocket_url = start_mock_remote_workflow_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: None,
                    }],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to remote gateway");

        let detected: ExternalAgentConfigDetectResponse = client
            .request_typed(ClientRequest::ExternalAgentConfigDetect {
                request_id: RequestId::Integer(1),
                params: ExternalAgentConfigDetectParams {
                    include_home: false,
                    cwds: Some(vec![PathBuf::from("/tmp/remote-project")]),
                },
            })
            .await
            .expect("externalAgentConfig/detect should succeed through remote gateway");
        assert_eq!(detected.items.is_empty(), true);

        let imported: ExternalAgentConfigImportResponse = client
            .request_typed(ClientRequest::ExternalAgentConfigImport {
                request_id: RequestId::Integer(2),
                params: ExternalAgentConfigImportParams {
                    migration_items: Vec::new(),
                },
            })
            .await
            .expect("externalAgentConfig/import should succeed through remote gateway");
        assert_eq!(imported, ExternalAgentConfigImportResponse {});

        let skills: SkillsListResponse = client
            .request_typed(ClientRequest::SkillsList {
                request_id: RequestId::Integer(3),
                params: SkillsListParams {
                    cwds: vec![PathBuf::from("/tmp/remote-project")],
                    force_reload: false,
                    per_cwd_extra_user_roots: None,
                },
            })
            .await
            .expect("skills/list should succeed through remote gateway");
        assert_eq!(skills.data.len(), 1);
        assert_eq!(skills.data[0].cwd, PathBuf::from("/tmp/remote-project"));
        assert_eq!(skills.data[0].errors, Vec::new());
        assert_eq!(skills.data[0].skills, Vec::new());

        let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
            "cwds": ["/tmp/remote-project"],
        }))
        .expect("plugin/list params should deserialize");
        let plugins: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(4),
                params: plugin_list_params,
            })
            .await
            .expect("plugin/list should succeed through remote gateway");
        assert_eq!(plugins.marketplaces.len(), 1);
        assert_eq!(plugins.marketplaces[0].name, "remote-marketplace");
        assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
        assert_eq!(plugins.marketplaces[0].plugins[0].name, "remote-plugin");
        assert_eq!(plugins.marketplaces[0].plugins[0].installed, false);

        let plugin_read_params: PluginReadParams = serde_json::from_value(serde_json::json!({
            "marketplacePath": "/tmp/remote-project/marketplace.json",
            "pluginName": "remote-plugin",
        }))
        .expect("plugin/read params should deserialize");
        let plugin: PluginReadResponse = client
            .request_typed(ClientRequest::PluginRead {
                request_id: RequestId::Integer(5),
                params: plugin_read_params,
            })
            .await
            .expect("plugin/read should succeed through remote gateway");
        assert_eq!(plugin.plugin.summary.name, "remote-plugin");
        assert_eq!(plugin.plugin.marketplace_name, "remote-marketplace");
        assert_eq!(plugin.plugin.skills.len(), 1);
        assert_eq!(plugin.plugin.skills[0].name, "remote-skill");

        let plugin_install_params: PluginInstallParams =
            serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/remote-project/marketplace.json",
                "pluginName": "remote-plugin",
            }))
            .expect("plugin/install params should deserialize");
        let install: PluginInstallResponse = client
            .request_typed(ClientRequest::PluginInstall {
                request_id: RequestId::Integer(6),
                params: plugin_install_params,
            })
            .await
            .expect("plugin/install should succeed through remote gateway");
        assert_eq!(
            install,
            PluginInstallResponse {
                auth_policy: PluginAuthPolicy::OnInstall,
                apps_needing_auth: Vec::new(),
            }
        );

        let plugins_after_install: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(7),
                params: serde_json::from_value(serde_json::json!({
                    "cwds": ["/tmp/remote-project"],
                }))
                .expect("plugin/list params after install should deserialize"),
            })
            .await
            .expect("plugin/list after install should succeed through remote gateway");
        assert_eq!(
            plugins_after_install.marketplaces[0].plugins[0].installed,
            true
        );

        let uninstall: PluginUninstallResponse = client
            .request_typed(ClientRequest::PluginUninstall {
                request_id: RequestId::Integer(8),
                params: PluginUninstallParams {
                    plugin_id: "remote-plugin@remote-marketplace".to_string(),
                },
            })
            .await
            .expect("plugin/uninstall should succeed through remote gateway");
        assert_eq!(uninstall, PluginUninstallResponse {});

        let plugins_after_uninstall: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(9),
                params: serde_json::from_value(serde_json::json!({
                    "cwds": ["/tmp/remote-project"],
                }))
                .expect("plugin/list params after uninstall should deserialize"),
            })
            .await
            .expect("plugin/list after uninstall should succeed through remote gateway");
        assert_eq!(
            plugins_after_uninstall.marketplaces[0].plugins[0].installed,
            false
        );

        let batch_write: ConfigWriteResponse = client
            .request_typed(ClientRequest::ConfigBatchWrite {
                request_id: RequestId::Integer(10),
                params: ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path: Some("/tmp/remote-project/config.toml".to_string()),
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await
            .expect("config/batchWrite should succeed through remote gateway");
        assert_eq!(batch_write.status, WriteStatus::Ok);
        assert_eq!(batch_write.version, "remote-version-1");
        assert_eq!(
            batch_write.file_path.as_path(),
            PathBuf::from("/tmp/remote-project/config.toml").as_path()
        );
        assert_eq!(batch_write.overridden_metadata, None);

        let config_value_write: ConfigWriteResponse = client
            .request_typed(ClientRequest::ConfigValueWrite {
                request_id: RequestId::Integer(11),
                params: ConfigValueWriteParams {
                    key_path: "plugins.remote-plugin".to_string(),
                    value: serde_json::json!({
                        "enabled": true,
                    }),
                    merge_strategy: MergeStrategy::Upsert,
                    file_path: None,
                    expected_version: None,
                },
            })
            .await
            .expect("config/value/write should succeed through remote gateway");
        assert_eq!(config_value_write.status, WriteStatus::Ok);
        assert_eq!(config_value_write.version, "remote-version-1");
        assert_eq!(
            config_value_write.file_path.as_path(),
            PathBuf::from("/tmp/remote-project/config.toml").as_path()
        );
        assert_eq!(config_value_write.overridden_metadata, None);

        let reset: MemoryResetResponse = client
            .request_typed(ClientRequest::MemoryReset {
                request_id: RequestId::Integer(12),
                params: None,
            })
            .await
            .expect("memory/reset should succeed through remote gateway");
        assert_eq!(reset, MemoryResetResponse {});

        let logout: LogoutAccountResponse = client
            .request_typed(ClientRequest::LogoutAccount {
                request_id: RequestId::Integer(13),
                params: None,
            })
            .await
            .expect("account/logout should succeed through remote gateway");
        assert_eq!(logout, LogoutAccountResponse {});

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_supports_server_request_roundtrip_over_v2() {
        let websocket_url = start_mock_remote_server_for_server_request_roundtrip().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: None,
                    }],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to remote gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/remote-project".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through remote gateway");
        assert_eq!(started.thread.id, "thread-remote-workflow");

        let (request_id, params) = timeout(Duration::from_secs(5), async {
            loop {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("server request should arrive");
        assert_eq!(params.thread_id, "thread-remote-workflow");
        assert_eq!(params.turn_id, "turn-remote-workflow");
        assert_eq!(params.item_id, "tool-call-remote-workflow");
        assert_eq!(params.questions.len(), 1);
        assert_eq!(params.questions[0].id, "mode");

        let mut answers = HashMap::new();
        answers.insert(
            "mode".to_string(),
            ToolRequestUserInputAnswer {
                answers: vec!["safe".to_string()],
            },
        );
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ToolRequestUserInputResponse { answers })
                    .expect("server request response should serialize"),
            )
            .await
            .expect("server request should resolve");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_supports_additional_server_request_roundtrips_over_v2() {
        let websocket_url = start_mock_remote_server_for_multiple_server_request_roundtrips().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: None,
                    }],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to remote gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/remote-project".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through remote gateway");
        assert_eq!(started.thread.id, "thread-remote-workflow");

        let mut saw_command_request = false;
        let mut saw_file_request = false;
        let mut saw_mcp_elicitation_request = false;
        let mut saw_permissions_request = false;
        let mut saw_refresh_request = false;

        timeout(Duration::from_secs(5), async {
            while !(saw_command_request
                && saw_file_request
                && saw_mcp_elicitation_request
                && saw_permissions_request
                && saw_refresh_request)
            {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(
                        ServerRequest::CommandExecutionRequestApproval { request_id, params },
                    ) => {
                        assert_eq!(params.thread_id, "thread-remote-workflow");
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "cmd-remote-workflow");
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(CommandExecutionRequestApprovalResponse {
                                    decision: CommandExecutionApprovalDecision::Accept,
                                })
                                .expect("command approval response should serialize"),
                            )
                            .await
                            .expect("command approval should resolve");
                        saw_command_request = true;
                    }
                    AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.thread_id, "thread-remote-workflow");
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "file-remote-workflow");
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(FileChangeRequestApprovalResponse {
                                    decision: FileChangeApprovalDecision::Accept,
                                })
                                .expect("file approval response should serialize"),
                            )
                            .await
                            .expect("file approval should resolve");
                        saw_file_request = true;
                    }
                    AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.thread_id, "thread-remote-workflow");
                        assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                        assert_eq!(params.server_name, "mock-mcp");
                        match params.request {
                            McpServerElicitationRequest::Form {
                                message,
                                requested_schema,
                                ..
                            } => {
                                assert_eq!(message, "Allow mock action?");
                                assert_eq!(
                                    serde_json::to_value(requested_schema)
                                        .expect("schema should serialize"),
                                    serde_json::json!({
                                        "type": "object",
                                        "properties": {
                                            "confirmed": {
                                                "type": "boolean",
                                            },
                                        },
                                        "required": ["confirmed"],
                                    })
                                );
                            }
                            other => panic!("unexpected elicitation request: {other:?}"),
                        }
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::json!({
                                    "action": "accept",
                                    "content": {
                                        "confirmed": true,
                                    },
                                    "_meta": null,
                                }),
                            )
                            .await
                            .expect("mcp elicitation should resolve");
                        saw_mcp_elicitation_request = true;
                    }
                    AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.thread_id, "thread-remote-workflow");
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "perm-remote-workflow");
                        assert_eq!(params.reason, Some("Need wider permissions".to_string()));
                        assert_eq!(
                            serde_json::to_value(params.permissions)
                                .expect("permissions should serialize"),
                            serde_json::json!({
                                "fileSystem": null,
                                "network": {
                                    "enabled": true,
                                },
                            })
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::json!({
                                    "permissions": {
                                        "fileSystem": null,
                                        "network": {
                                            "enabled": true,
                                        },
                                    },
                                    "scope": "turn",
                                }),
                            )
                            .await
                            .expect("permissions approval should resolve");
                        saw_permissions_request = true;
                    }
                    AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.previous_account_id, Some("acct-123".to_string()));
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                    access_token: "access-token-1".to_string(),
                                    chatgpt_account_id: "acct-123".to_string(),
                                    chatgpt_plan_type: Some("pro".to_string()),
                                })
                                .expect("chatgpt refresh response should serialize"),
                            )
                            .await
                            .expect("chatgpt refresh should resolve");
                        saw_refresh_request = true;
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("all additional server requests should arrive");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_v2_clients_recover_after_worker_reconnect() {
        let websocket_url =
            start_reconnecting_v2_mock_remote_server(Some("secret-token".to_string())).await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url: websocket_url.clone(),
                        auth_token: Some("secret-token".to_string()),
                    }],
                }),
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
        timeout(Duration::from_secs(5), async {
            loop {
                let healthz_response = client
                    .get(format!("http://{}/healthz", server.local_addr()))
                    .send()
                    .await
                    .expect("healthz response");
                let health: GatewayHealthResponse =
                    healthz_response.json().await.expect("health body");
                if health.status == GatewayHealthStatus::Ok
                    && health
                        .remote_workers
                        .as_ref()
                        .and_then(|workers| workers.first())
                        .and_then(|worker| worker.last_error.as_ref())
                        .is_some()
                {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("worker should reconnect before v2 client connects");

        let mut second_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("v2 client should connect after worker reconnect");

        let second_started: AppServerThreadStartResponse = second_client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed after worker reconnect");
        assert_eq!(second_started.thread.id, "thread-worker-a-2");

        let (request_id, params) = timeout(Duration::from_secs(10), async {
            loop {
                let event = second_client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) = event
                {
                    break (request_id, params);
                }
            }
        })
        .await
        .expect("server request should arrive after worker reconnect");
        assert_eq!(params.thread_id, "thread-worker-a-2");
        assert_eq!(params.turn_id, "turn-worker-a-2");
        assert_eq!(params.item_id, "tool-call-worker-a-2");

        let mut answers = HashMap::new();
        answers.insert(
            "mode".to_string(),
            ToolRequestUserInputAnswer {
                answers: vec!["safe".to_string()],
            },
        );
        second_client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ToolRequestUserInputResponse { answers })
                    .expect("server request response should serialize"),
            )
            .await
            .expect("server request should resolve after worker reconnect");

        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.status, GatewayHealthStatus::Ok);
        assert_eq!(
            health.v2_compatibility,
            GatewayV2CompatibilityMode::RemoteSingleWorker
        );

        assert_remote_client_shutdown(second_client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_v2_clients_recover_after_worker_reconnect() {
        let worker_a = start_reconnecting_v2_multi_connection_thread_server(
            "thread-worker-a-2",
            "/tmp/worker-a-2",
        )
        .await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
        timeout(Duration::from_secs(5), async {
            loop {
                let healthz_response = client
                    .get(format!("http://{}/healthz", server.local_addr()))
                    .send()
                    .await
                    .expect("healthz response");
                let health: GatewayHealthResponse =
                    healthz_response.json().await.expect("health body");
                let Some(remote_workers) = health.remote_workers.as_ref() else {
                    panic!("remote workers should exist");
                };
                if health.status == GatewayHealthStatus::Ok
                    && remote_workers[0].healthy
                    && remote_workers[0].last_error.is_some()
                    && remote_workers[1].healthy
                {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("worker should reconnect before v2 client connects");

        let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("v2 client should connect after worker reconnect");

        let first_started: AppServerThreadStartResponse = v2_client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed after worker reconnect");
        assert_eq!(first_started.thread.id, "thread-worker-a-2");

        let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
            request_id,
            params,
        }) = timeout(Duration::from_secs(5), v2_client.next_event())
            .await
            .expect("server request should arrive after multi-worker reconnect")
            .expect("event stream should stay open after multi-worker reconnect")
        else {
            panic!("expected tool/requestUserInput after multi-worker reconnect");
        };
        assert_eq!(params.thread_id, "thread-worker-a-2");
        assert_eq!(params.turn_id, "turn-worker-a-2");
        assert_eq!(params.item_id, "tool-call-worker-a-2");
        let mut answers = HashMap::new();
        answers.insert(
            "mode".to_string(),
            ToolRequestUserInputAnswer {
                answers: vec!["safe".to_string()],
            },
        );
        v2_client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ToolRequestUserInputResponse { answers })
                    .expect("server request response should serialize"),
            )
            .await
            .expect("server request should resolve after multi-worker reconnect");

        let recovered_turn_started: TurnStartResponse = v2_client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(2),
                params: TurnStartParams {
                    thread_id: first_started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "hello recovered worker".to_string(),
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
                },
            })
            .await
            .expect("turn/start should succeed after multi-worker reconnect");
        assert_eq!(recovered_turn_started.turn.id, "turn-thread-worker-a-2");
        assert_eq!(recovered_turn_started.turn.status, TurnStatus::InProgress);

        let mut recovered_turn_coverage = TurnStreamingCoverage::default();
        timeout(Duration::from_secs(5), async {
            while recovered_turn_coverage
                != (TurnStreamingCoverage {
                    saw_thread_active: true,
                    saw_turn_started: true,
                    saw_agent_delta: true,
                    saw_reasoning_summary_delta: true,
                    saw_reasoning_text_delta: true,
                    saw_command_output_delta: true,
                    saw_file_change_delta: true,
                    saw_turn_completed: true,
                })
            {
                let event = v2_client
                    .next_event()
                    .await
                    .expect("event stream should stay open after multi-worker reconnect");
                match event {
                    AppServerEvent::ServerNotification(
                        ServerNotification::ThreadStatusChanged(notification),
                    ) if notification.thread_id == first_started.thread.id
                        && matches!(
                            notification.status,
                            ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                        ) =>
                    {
                        recovered_turn_coverage.saw_thread_active = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                        notification,
                    )) if notification.thread_id == first_started.thread.id
                        && notification.turn.id == "turn-thread-worker-a-2" =>
                    {
                        recovered_turn_coverage.saw_turn_started = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                        notification,
                    )) if notification.thread_id == first_started.thread.id
                        && notification.turn_id == "turn-thread-worker-a-2"
                        && notification.delta == "hello from recovered worker" =>
                    {
                        recovered_turn_coverage.saw_agent_delta = true;
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ReasoningSummaryTextDelta(notification),
                    ) if notification.thread_id == first_started.thread.id
                        && notification.turn_id == "turn-thread-worker-a-2"
                        && notification.delta == "summary thread-worker-a-2"
                        && notification.summary_index == 0 =>
                    {
                        recovered_turn_coverage.saw_reasoning_summary_delta = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                        notification,
                    )) if notification.thread_id == first_started.thread.id
                        && notification.turn_id == "turn-thread-worker-a-2"
                        && notification.delta == "reasoning thread-worker-a-2"
                        && notification.content_index == 0 =>
                    {
                        recovered_turn_coverage.saw_reasoning_text_delta = true;
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::CommandExecutionOutputDelta(notification),
                    ) if notification.thread_id == first_started.thread.id
                        && notification.turn_id == "turn-thread-worker-a-2"
                        && notification.delta == "stdout thread-worker-a-2" =>
                    {
                        recovered_turn_coverage.saw_command_output_delta = true;
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::FileChangeOutputDelta(notification),
                    ) if notification.thread_id == first_started.thread.id
                        && notification.turn_id == "turn-thread-worker-a-2"
                        && notification.delta == "patch thread-worker-a-2" =>
                    {
                        recovered_turn_coverage.saw_file_change_delta = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                        notification,
                    )) if notification.thread_id == first_started.thread.id
                        && notification.turn.id == "turn-thread-worker-a-2"
                        && notification.turn.status == TurnStatus::Completed =>
                    {
                        recovered_turn_coverage.saw_turn_completed = true;
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("turn notifications should fan in after multi-worker reconnect");

        let second_started: AppServerThreadStartResponse = v2_client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(3),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed after worker reconnect");
        assert_eq!(second_started.thread.id, "thread-worker-b");

        let listed: AppServerThreadListResponse = v2_client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(4),
                params: ThreadListParams {
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
            .expect("thread/list should succeed after worker reconnect");
        assert_eq!(listed.next_cursor, None);
        assert_eq!(
            listed
                .data
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-worker-b", "thread-worker-a-2"]
        );

        let first_read: AppServerThreadReadResponse = v2_client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(5),
                params: ThreadReadParams {
                    thread_id: first_started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read should route to recovered worker");
        assert_eq!(first_read.thread.id, "thread-worker-a-2");
        assert_eq!(first_read.thread.preview, "/tmp/worker-a-2");

        assert_remote_client_shutdown(v2_client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_deduplicates_connection_state_notifications_after_worker_reconnect()
     {
        let worker_a = start_reconnecting_v2_multi_connection_state_notification_server().await;
        let worker_b = start_mock_remote_multi_connection_state_notification_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
        timeout(Duration::from_secs(5), async {
            loop {
                let healthz_response = client
                    .get(format!("http://{}/healthz", server.local_addr()))
                    .send()
                    .await
                    .expect("healthz response");
                let health: GatewayHealthResponse =
                    healthz_response.json().await.expect("health body");
                let Some(remote_workers) = health.remote_workers.as_ref() else {
                    panic!("remote workers should exist");
                };
                if health.status == GatewayHealthStatus::Ok
                    && remote_workers[0].healthy
                    && remote_workers[0].last_error.is_some()
                    && remote_workers[1].healthy
                {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("worker should reconnect before v2 client connects");

        let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("v2 client should connect after worker reconnect");

        let mut saw_account_updated = false;
        let mut saw_rate_limits = false;
        let mut saw_app_list = false;
        timeout(Duration::from_secs(5), async {
            while !(saw_account_updated && saw_rate_limits && saw_app_list) {
                let event = v2_client
                    .next_event()
                    .await
                    .expect("event stream should stay open after worker reconnect");
                match event {
                    AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                        notification,
                    )) => {
                        assert_eq!(notification.auth_mode, None);
                        saw_account_updated = true;
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::AccountRateLimitsUpdated(notification),
                    ) => {
                        assert_eq!(notification.rate_limits.plan_type, None);
                        saw_rate_limits = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AppListUpdated(
                        notification,
                    )) => {
                        assert_eq!(notification.data.len(), 1);
                        assert_eq!(notification.data[0].name, "calendar");
                        saw_app_list = true;
                    }
                    other => panic!("unexpected notification after worker reconnect: {other:?}"),
                }
            }
        })
        .await
        .expect("deduplicated state notifications should arrive after reconnect");
        assert!(saw_account_updated);
        assert!(saw_rate_limits);
        assert!(saw_app_list);
        assert!(
            timeout(Duration::from_millis(200), v2_client.next_event())
                .await
                .is_err(),
            "duplicate connection-state notifications should still be suppressed after reconnect"
        );

        assert_remote_client_shutdown(v2_client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_deduplicates_skills_changed_notifications_after_worker_reconnect()
    {
        let worker_a = start_reconnecting_v2_multi_connection_skills_changed_server().await;
        let worker_b = start_mock_remote_multi_connection_skills_changed_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
        timeout(Duration::from_secs(5), async {
            loop {
                let healthz_response = client
                    .get(format!("http://{}/healthz", server.local_addr()))
                    .send()
                    .await
                    .expect("healthz response");
                let health: GatewayHealthResponse =
                    healthz_response.json().await.expect("health body");
                let Some(remote_workers) = health.remote_workers.as_ref() else {
                    panic!("remote workers should exist");
                };
                if health.status == GatewayHealthStatus::Ok
                    && remote_workers[0].healthy
                    && remote_workers[0].last_error.is_some()
                    && remote_workers[1].healthy
                {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("worker should reconnect before v2 client connects");

        let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("v2 client should connect after worker reconnect");

        let first_notification = timeout(Duration::from_secs(5), v2_client.next_event())
            .await
            .expect("initial skills/changed should finish in time after reconnect")
            .expect("event stream should stay open after reconnect");
        assert!(matches!(
            first_notification,
            AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
        ));
        assert!(
            timeout(Duration::from_millis(200), v2_client.next_event())
                .await
                .is_err(),
            "duplicate initial skills/changed should be suppressed after reconnect"
        );

        let _: SkillsListResponse = timeout(
            Duration::from_secs(5),
            v2_client.request_typed(ClientRequest::SkillsList {
                request_id: RequestId::Integer(1),
                params: SkillsListParams {
                    cwds: vec!["/tmp/shared-repo".into()],
                    force_reload: true,
                    per_cwd_extra_user_roots: None,
                },
            }),
        )
        .await
        .expect("skills/list should finish in time after reconnect")
        .expect("skills/list should succeed through multi-worker gateway after reconnect");

        let second_notification = timeout(Duration::from_secs(5), v2_client.next_event())
            .await
            .expect("post-refresh skills/changed should finish in time after reconnect")
            .expect("event stream should stay open after reconnect");
        assert!(matches!(
            second_notification,
            AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
        ));
        assert!(
            timeout(Duration::from_millis(200), v2_client.next_event())
                .await
                .is_err(),
            "duplicate post-refresh skills/changed should be suppressed after reconnect"
        );

        assert_remote_client_shutdown(v2_client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_single_worker_preserves_unmaterialized_thread_resume_and_fork_errors_over_v2() {
        let websocket_url = start_mock_remote_workflow_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url,
                        auth_token: None,
                    }],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to remote gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/remote-project".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("thread/start should succeed through remote gateway");

        let resume_error = client
            .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
                request_id: RequestId::Integer(2),
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
                    persist_extended_history: false,
                },
            })
            .await
            .expect_err("unmaterialized thread/resume should fail through remote gateway");
        assert_eq!(
            resume_error.to_string(),
            format!(
                "thread/resume failed: no rollout found for thread id {}",
                started.thread.id
            )
        );
        let TypedRequestError::Server {
            source: resume_source,
            ..
        } = resume_error
        else {
            panic!("thread/resume should return a server JSON-RPC error");
        };
        assert_eq!(
            resume_source.message,
            format!("no rollout found for thread id {}", started.thread.id)
        );

        let fork_error = client
            .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
                request_id: RequestId::Integer(3),
                params: ThreadForkParams {
                    thread_id: started.thread.id.clone(),
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
                    ephemeral: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect_err("unmaterialized thread/fork should fail through remote gateway");
        assert_eq!(
            fork_error.to_string(),
            format!(
                "thread/fork failed: no rollout found for thread id {}",
                started.thread.id
            )
        );
        let TypedRequestError::Server {
            source: fork_source,
            ..
        } = fork_error
        else {
            panic!("thread/fork should return a server JSON-RPC error");
        };
        assert_eq!(
            fork_source.message,
            format!("no rollout found for thread id {}", started.thread.id)
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_round_robins_threads_across_workers() {
        let worker_a =
            start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a")
                .await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-a".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("first response");
        assert_eq!(first_response.status(), reqwest::StatusCode::OK);
        let first_thread: ThreadResponse = first_response.json().await.expect("first thread");

        let second_response = client
            .post(format!("http://{}/v1/threads", server.local_addr()))
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-b".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("second response");
        assert_eq!(second_response.status(), reqwest::StatusCode::OK);
        let second_thread: ThreadResponse = second_response.json().await.expect("second thread");

        assert_eq!(first_thread.thread.id, "thread-worker-a");
        assert_eq!(second_thread.thread.id, "thread-worker-b");

        let list_response = client
            .get(format!(
                "http://{}/v1/threads?limit=10",
                server.local_addr()
            ))
            .send()
            .await
            .expect("list response");
        assert_eq!(list_response.status(), reqwest::StatusCode::OK);
        let list: ListThreadsResponse = list_response.json().await.expect("thread list");
        assert_eq!(list.data.len(), 2);
        assert_eq!(
            list.data
                .into_iter()
                .map(|thread| thread.id)
                .collect::<Vec<_>>(),
            vec!["thread-worker-b".to_string(), "thread-worker-a".to_string()]
        );

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_skips_unhealthy_workers_for_new_threads() {
        let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
            expected_auth_token: Some("secret-token".to_string()),
            thread_id: "thread-worker-a",
            preview: "/tmp/worker-a",
            close_after_first_request: true,
        })
        .await;
        let worker_b = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-b",
            "/tmp/worker-b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: Some("secret-token".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: Some("secret-token".to_string()),
                        },
                    ],
                }),
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
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-a".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("first response");
        assert_eq!(first_response.status(), reqwest::StatusCode::OK);
        let first_thread: ThreadResponse = first_response.json().await.expect("first thread");
        assert_eq!(first_thread.thread.id, "thread-worker-a");

        sleep(Duration::from_millis(100)).await;

        let second_response = client
            .post(format!("http://{}/v1/threads", server.local_addr()))
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-b".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("second response");
        assert_eq!(second_response.status(), reqwest::StatusCode::OK);
        let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
        assert_eq!(second_thread.thread.id, "thread-worker-b");

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_returns_bad_gateway_for_threads_on_unhealthy_workers() {
        let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
            expected_auth_token: Some("secret-token".to_string()),
            thread_id: "thread-worker-a",
            preview: "/tmp/worker-a",
            close_after_first_request: true,
        })
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,
                        auth_token: Some("secret-token".to_string()),
                    }],
                }),
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
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-a".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("create response");
        assert_eq!(create_response.status(), reqwest::StatusCode::OK);
        let thread: ThreadResponse = create_response.json().await.expect("thread");

        sleep(Duration::from_millis(100)).await;

        let read_response = client
            .get(format!(
                "http://{}/v1/threads/{}",
                server.local_addr(),
                thread.thread.id
            ))
            .send()
            .await
            .expect("read response");
        assert_eq!(read_response.status(), reqwest::StatusCode::BAD_GATEWAY);
        assert_eq!(
            read_response
                .text()
                .await
                .expect("body")
                .contains("unhealthy"),
            true
        );

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_healthz_reports_degraded_workers() {
        let worker_a = start_mock_remote_server_with_options(MockRemoteServerOptions {
            expected_auth_token: Some("secret-token".to_string()),
            thread_id: "thread-worker-a",
            preview: "/tmp/worker-a",
            close_after_first_request: true,
        })
        .await;
        let worker_b = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-b",
            "/tmp/worker-b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a.clone(),
                            auth_token: Some("secret-token".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b.clone(),
                            auth_token: Some("secret-token".to_string()),
                        },
                    ],
                }),
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
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-a".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("create response");
        assert_eq!(create_response.status(), reqwest::StatusCode::OK);

        sleep(Duration::from_millis(100)).await;

        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.status, GatewayHealthStatus::Degraded);
        assert_eq!(health.runtime_mode, "remote");
        assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
        let remote_workers = health.remote_workers.expect("remote workers");
        assert_eq!(remote_workers.len(), 2);
        assert_eq!(remote_workers[0].worker_id, 0);
        assert_eq!(remote_workers[0].websocket_url, worker_a);
        assert_eq!(remote_workers[0].healthy, false);
        assert_eq!(
            remote_workers[0]
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("remote app server")),
            true
        );
        assert_eq!(remote_workers[1].worker_id, 1);
        assert_eq!(remote_workers[1].websocket_url, worker_b);
        assert_eq!(remote_workers[1].healthy, true);
        assert_eq!(remote_workers[1].last_error, None);

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_runtime_healthz_reports_v2_as_supported() {
        let worker_a = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-a",
            "/tmp/worker-a",
        )
        .await;
        let worker_b = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-b",
            "/tmp/worker-b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: Some("secret-token".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: Some("secret-token".to_string()),
                        },
                    ],
                }),
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
        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.status, GatewayHealthStatus::Ok);
        assert_eq!(health.runtime_mode, "remote");
        assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
        assert_eq!(
            health.v2_compatibility,
            GatewayV2CompatibilityMode::RemoteMultiWorker
        );
        assert_eq!(health.remote_workers.as_ref().map(Vec::len), Some(2));

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_runtime_accepts_v2_websocket_upgrades() {
        let worker_a = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-a",
            "/tmp/worker-a",
        )
        .await;
        let worker_b = start_mock_remote_server(
            Some("secret-token".to_string()),
            "thread-worker-b",
            "/tmp/worker-b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: Some("secret-token".to_string()),
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: Some("secret-token".to_string()),
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let request = format!("ws://{}/", server.local_addr())
            .into_client_request()
            .expect("request should build");
        let (mut websocket, response) = connect_async(request)
            .await
            .expect("websocket upgrade should succeed for remote multi-worker v2");
        assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);
        websocket.close(None).await.expect("websocket should close");

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_supports_v2_client_thread_routing_and_aggregation() {
        let worker_a =
            start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a")
                .await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        assert_eq!(first_started.thread.id, "thread-worker-a");

        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed through multi-worker remote gateway");
        assert_eq!(second_started.thread.id, "thread-worker-b");

        let listed: AppServerThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(3),
                params: ThreadListParams {
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
            .expect("thread/list should succeed through multi-worker remote gateway");
        assert_eq!(listed.next_cursor, None);
        assert_eq!(
            listed
                .data
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-worker-b", "thread-worker-a"]
        );

        let loaded: ThreadLoadedListResponse = client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(4),
                params: ThreadLoadedListParams {
                    cursor: None,
                    limit: Some(10),
                },
            })
            .await
            .expect("thread/loaded/list should succeed through multi-worker remote gateway");
        assert_eq!(loaded.next_cursor, None);
        assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);

        let first_read: AppServerThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(5),
                params: ThreadReadParams {
                    thread_id: first_started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read should route to worker A");
        assert_eq!(first_read.thread.id, first_started.thread.id);
        assert_eq!(
            first_read.thread.cwd.as_ref().to_string_lossy(),
            "/tmp/worker-a"
        );

        let second_read: AppServerThreadReadResponse = client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(6),
                params: ThreadReadParams {
                    thread_id: second_started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("thread/read should route to worker B");
        assert_eq!(second_read.thread.id, second_started.thread.id);
        assert_eq!(
            second_read.thread.cwd.as_ref().to_string_lossy(),
            "/tmp/worker-b"
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_v2_scope_filters_threads_across_tenants_and_projects() {
        let worker_a =
            start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a")
                .await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let owner_client = RemoteAppServerClient::connect_with_headers(
            RemoteAppServerConnectArgs {
                websocket_url: format!("ws://{}/", server.local_addr()),
                auth_token: None,
                client_name: "codex-gateway-test".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: true,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            vec![
                ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
                ("x-codex-project-id".to_string(), "project-a".to_string()),
            ],
        )
        .await
        .expect("owner client should connect to multi-worker gateway");

        let first_started: AppServerThreadStartResponse = owner_client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        assert_eq!(first_started.thread.id, "thread-worker-a");

        let second_started: AppServerThreadStartResponse = owner_client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed through multi-worker remote gateway");
        assert_eq!(second_started.thread.id, "thread-worker-b");

        let same_scope_client = RemoteAppServerClient::connect_with_headers(
            RemoteAppServerConnectArgs {
                websocket_url: format!("ws://{}/", server.local_addr()),
                auth_token: None,
                client_name: "codex-gateway-test".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: true,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            vec![
                ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
                ("x-codex-project-id".to_string(), "project-a".to_string()),
            ],
        )
        .await
        .expect("same-scope client should connect to multi-worker gateway");

        let same_scope_list: AppServerThreadListResponse = same_scope_client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(3),
                params: ThreadListParams {
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
            .expect("same-scope thread/list should succeed through multi-worker remote gateway");
        assert_eq!(same_scope_list.next_cursor, None);
        assert_eq!(
            same_scope_list
                .data
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-worker-b", "thread-worker-a"]
        );

        let same_scope_loaded: ThreadLoadedListResponse = same_scope_client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(4),
                params: ThreadLoadedListParams {
                    cursor: None,
                    limit: Some(10),
                },
            })
            .await
            .expect("same-scope thread/loaded/list should succeed through multi-worker gateway");
        assert_eq!(same_scope_loaded.next_cursor, None);
        assert_eq!(
            same_scope_loaded.data,
            vec!["thread-worker-a", "thread-worker-b"]
        );

        let same_scope_read: AppServerThreadReadResponse = same_scope_client
            .request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(5),
                params: ThreadReadParams {
                    thread_id: first_started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect("same-scope thread/read should succeed through multi-worker gateway");
        assert_eq!(same_scope_read.thread.id, first_started.thread.id);

        let other_project_client = RemoteAppServerClient::connect_with_headers(
            RemoteAppServerConnectArgs {
                websocket_url: format!("ws://{}/", server.local_addr()),
                auth_token: None,
                client_name: "codex-gateway-test".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: true,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            vec![
                ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
                ("x-codex-project-id".to_string(), "project-b".to_string()),
            ],
        )
        .await
        .expect("other-project client should connect to multi-worker gateway");

        let other_project_list: AppServerThreadListResponse = other_project_client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(6),
                params: ThreadListParams {
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
            .expect("other-project thread/list should succeed through multi-worker gateway");
        assert_eq!(other_project_list.data.is_empty(), true);

        let other_project_loaded: ThreadLoadedListResponse = other_project_client
            .request_typed(ClientRequest::ThreadLoadedList {
                request_id: RequestId::Integer(7),
                params: ThreadLoadedListParams {
                    cursor: None,
                    limit: Some(10),
                },
            })
            .await
            .expect("other-project thread/loaded/list should succeed through multi-worker gateway");
        assert_eq!(other_project_loaded.data.is_empty(), true);

        let other_project_read_error = other_project_client
            .request_typed::<AppServerThreadReadResponse>(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(8),
                params: ThreadReadParams {
                    thread_id: first_started.thread.id.clone(),
                    include_turns: false,
                },
            })
            .await
            .expect_err("other-project thread/read should be scope-filtered");
        assert_eq!(
            other_project_read_error.to_string(),
            format!(
                "thread/read failed: thread not found: {}",
                first_started.thread.id
            )
        );
        let TypedRequestError::Server {
            source: other_project_read_source,
            ..
        } = other_project_read_error
        else {
            panic!("other-project thread/read should return a server JSON-RPC error");
        };
        assert_eq!(
            other_project_read_source.message,
            format!("thread not found: {}", first_started.thread.id)
        );

        let other_tenant_client = RemoteAppServerClient::connect_with_headers(
            RemoteAppServerConnectArgs {
                websocket_url: format!("ws://{}/", server.local_addr()),
                auth_token: None,
                client_name: "codex-gateway-test".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: true,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            vec![
                ("x-codex-tenant-id".to_string(), "tenant-b".to_string()),
                ("x-codex-project-id".to_string(), "project-a".to_string()),
            ],
        )
        .await
        .expect("other-tenant client should connect to multi-worker gateway");

        let other_tenant_list: AppServerThreadListResponse = other_tenant_client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(9),
                params: ThreadListParams {
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
            .expect("other-tenant thread/list should succeed through multi-worker gateway");
        assert_eq!(other_tenant_list.data.is_empty(), true);

        assert_remote_client_shutdown(owner_client.shutdown().await);
        assert_remote_client_shutdown(same_scope_client.shutdown().await);
        assert_remote_client_shutdown(other_project_client.shutdown().await);
        assert_remote_client_shutdown(other_tenant_client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_v2_session_survives_one_worker_disconnect() {
        let worker_a = start_mock_remote_multi_connection_disconnect_after_thread_start_server(
            "thread-worker-a",
            "/tmp/worker-a",
        )
        .await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        assert_eq!(first_started.thread.id, "thread-worker-a");

        sleep(Duration::from_millis(200)).await;

        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should still succeed after one worker disconnects");
        assert_eq!(second_started.thread.id, "thread-worker-b");

        let listed: AppServerThreadListResponse = client
            .request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(3),
                params: ThreadListParams {
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
            .expect("thread/list should still succeed after one worker disconnects");
        assert_eq!(listed.next_cursor, None);
        assert_eq!(
            listed
                .data
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            vec!["thread-worker-b"]
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_resolves_pending_thread_scoped_server_requests_when_worker_disconnects()
     {
        let worker_a =
            start_mock_remote_multi_connection_disconnect_after_server_request_server().await;
        let worker_b =
            start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        assert_eq!(started.thread.id, "thread-worker-a");

        let server_request_id = timeout(Duration::from_secs(5), async {
            match client
                .next_event()
                .await
                .expect("event stream should stay open")
            {
                AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a");
                    assert_eq!(params.turn_id, "turn-worker-a");
                    assert_eq!(params.item_id, "tool-call-worker-a");
                    request_id
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    _,
                )) => {
                    panic!("serverRequest/resolved should follow the server request")
                }
                other => panic!("unexpected event: {other:?}"),
            }
        })
        .await
        .expect("thread-scoped server request should arrive");

        timeout(Duration::from_secs(5), async {
            match client
                .next_event()
                .await
                .expect("event stream should stay open")
            {
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    notification,
                )) => {
                    assert_eq!(notification.thread_id, "thread-worker-a");
                    assert_eq!(notification.request_id, server_request_id);
                }
                other => panic!("unexpected event: {other:?}"),
            }
        })
        .await
        .expect("gateway should resolve the disconnected worker's pending request");

        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should still succeed after one worker disconnects");
        assert_eq!(second_started.thread.id, "thread-worker-b");

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_fanouts_connection_mutations_over_v2() {
        let (worker_a, worker_a_requests) =
            start_mock_remote_multi_connection_session_mutation_server("worker-a").await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_multi_connection_session_mutation_server("worker-b").await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let imported: ExternalAgentConfigImportResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ExternalAgentConfigImport {
                request_id: RequestId::Integer(1),
                params: ExternalAgentConfigImportParams {
                    migration_items: Vec::new(),
                },
            }),
        )
        .await
        .expect("externalAgentConfig/import should finish in time")
        .expect("externalAgentConfig/import should fan out through multi-worker gateway");
        assert_eq!(imported, ExternalAgentConfigImportResponse {});

        let batch_write: ConfigWriteResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ConfigBatchWrite {
                request_id: RequestId::Integer(2),
                params: ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path: Some("/tmp/shared/config.toml".to_string()),
                    expected_version: None,
                    reload_user_config: true,
                },
            }),
        )
        .await
        .expect("config/batchWrite should finish in time")
        .expect("config/batchWrite should fan out through multi-worker gateway");
        assert_eq!(batch_write.version, "worker-a");

        let config_value_write: ConfigWriteResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ConfigValueWrite {
                request_id: RequestId::Integer(3),
                params: ConfigValueWriteParams {
                    key_path: "plugins.shared-plugin".to_string(),
                    value: serde_json::json!({
                        "enabled": true,
                    }),
                    merge_strategy: MergeStrategy::Upsert,
                    file_path: None,
                    expected_version: None,
                },
            }),
        )
        .await
        .expect("config/value/write should finish in time")
        .expect("config/value/write should fan out through multi-worker gateway");
        assert_eq!(config_value_write.version, "worker-a");

        let reset: MemoryResetResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::MemoryReset {
                request_id: RequestId::Integer(4),
                params: None,
            }),
        )
        .await
        .expect("memory/reset should finish in time")
        .expect("memory/reset should fan out through multi-worker gateway");
        assert_eq!(reset, MemoryResetResponse {});

        let logout: LogoutAccountResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::LogoutAccount {
                request_id: RequestId::Integer(5),
                params: None,
            }),
        )
        .await
        .expect("account/logout should finish in time")
        .expect("account/logout should fan out through multi-worker gateway");
        assert_eq!(logout, LogoutAccountResponse {});

        let expected_methods = vec![
            "externalAgentConfig/import".to_string(),
            "config/batchWrite".to_string(),
            "config/value/write".to_string(),
            "memory/reset".to_string(),
            "account/logout".to_string(),
        ];
        assert_eq!(*worker_a_requests.lock().await, expected_methods);
        assert_eq!(*worker_b_requests.lock().await, expected_methods);

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_aggregates_setup_discovery_requests_over_v2() {
        let worker_a = start_mock_remote_multi_connection_discovery_server(
            "worker-a",
            "/tmp/shared-repo",
            "/tmp/worker-a-only",
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_discovery_server(
            "worker-b",
            "/tmp/shared-repo",
            "/tmp/worker-b-only",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let detected: ExternalAgentConfigDetectResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ExternalAgentConfigDetect {
                request_id: RequestId::Integer(1),
                params: ExternalAgentConfigDetectParams {
                    include_home: true,
                    cwds: Some(vec!["/tmp/shared-repo".into()]),
                },
            }),
        )
        .await
        .expect("externalAgentConfig/detect should finish in time")
        .expect("externalAgentConfig/detect should aggregate through multi-worker gateway");
        assert_eq!(detected.items.len(), 3);
        assert_eq!(
            detected
                .items
                .iter()
                .map(|item| item.description.as_str())
                .collect::<Vec<_>>(),
            vec![
                "shared config",
                "worker-a repo config",
                "worker-b repo config",
            ]
        );

        let skills: SkillsListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::SkillsList {
                request_id: RequestId::Integer(2),
                params: SkillsListParams {
                    cwds: vec!["/tmp/shared-repo".into()],
                    force_reload: false,
                    per_cwd_extra_user_roots: None,
                },
            }),
        )
        .await
        .expect("skills/list should finish in time")
        .expect("skills/list should aggregate through multi-worker gateway");
        assert_eq!(skills.data.len(), 1);
        assert_eq!(skills.data[0].cwd, PathBuf::from("/tmp/shared-repo"));
        assert_eq!(
            skills.data[0]
                .skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-skill", "worker-a-skill", "worker-b-skill"]
        );
        assert_eq!(
            skills.data[0]
                .errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>(),
            vec!["shared warning", "worker-a warning", "worker-b warning"]
        );

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_aggregates_bootstrap_account_and_models_over_v2() {
        let worker_a = start_mock_remote_multi_connection_bootstrap_server(
            false,
            Some(vec![("codex", "Codex", 20), ("shared", "Shared", 5)]),
            vec![
                ("shared-model", "Shared Model", true),
                ("worker-a-model", "Worker A Model", false),
            ],
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_bootstrap_server(
            true,
            Some(vec![("codex", "Codex", 20), ("worker-b", "Worker B", 35)]),
            vec![
                ("shared-model", "Shared Model", true),
                ("worker-b-model", "Worker B Model", false),
            ],
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let account: GetAccountResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::GetAccount {
                request_id: RequestId::Integer(1),
                params: GetAccountParams {
                    refresh_token: false,
                },
            }),
        )
        .await
        .expect("account/read should finish in time")
        .expect("account/read should aggregate through multi-worker gateway");
        assert_eq!(account.account, None);
        assert_eq!(account.requires_openai_auth, true);

        let rate_limits: GetAccountRateLimitsResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::GetAccountRateLimits {
                request_id: RequestId::Integer(2),
                params: None,
            }),
        )
        .await
        .expect("account/rateLimits/read should finish in time")
        .expect("account/rateLimits/read should aggregate through multi-worker gateway");
        assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
        assert_eq!(
            rate_limits
                .rate_limits_by_limit_id
                .expect("aggregated rate limits should include per-limit map")
                .keys()
                .map(String::as_str)
                .collect::<HashSet<_>>(),
            HashSet::from(["codex", "shared", "worker-b"])
        );

        let models: ModelListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ModelList {
                request_id: RequestId::Integer(3),
                params: ModelListParams {
                    cursor: None,
                    limit: None,
                    include_hidden: Some(true),
                },
            }),
        )
        .await
        .expect("model/list should finish in time")
        .expect("model/list should aggregate through multi-worker gateway");
        assert_eq!(models.next_cursor, None);
        assert_eq!(
            models
                .data
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-model", "worker-a-model", "worker-b-model"]
        );
        assert_eq!(
            models
                .data
                .iter()
                .filter(|model| model.is_default)
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-model"]
        );

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_aggregates_apps_list_over_v2() {
        let worker_a = start_mock_remote_multi_connection_apps_list_server(vec![
            ("shared-app", "Shared App"),
            ("worker-a-app", "Worker A App"),
        ])
        .await;
        let worker_b = start_mock_remote_multi_connection_apps_list_server(vec![
            ("shared-app", "Shared App"),
            ("worker-b-app", "Worker B App"),
        ])
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_page: AppsListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::AppsList {
                request_id: RequestId::Integer(1),
                params: AppsListParams {
                    cursor: None,
                    limit: Some(2),
                    thread_id: None,
                    force_refetch: false,
                },
            }),
        )
        .await
        .expect("app/list first page should finish in time")
        .expect("app/list first page should aggregate through multi-worker gateway");
        assert_eq!(
            first_page
                .data
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-app", "worker-a-app"]
        );
        assert_eq!(first_page.next_cursor.as_deref(), Some("apps-offset:2"));

        let second_page: AppsListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::AppsList {
                request_id: RequestId::Integer(2),
                params: AppsListParams {
                    cursor: first_page.next_cursor.clone(),
                    limit: Some(2),
                    thread_id: None,
                    force_refetch: false,
                },
            }),
        )
        .await
        .expect("app/list second page should finish in time")
        .expect("app/list second page should aggregate through multi-worker gateway");
        assert_eq!(
            second_page
                .data
                .iter()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            vec!["worker-b-app"]
        );
        assert_eq!(second_page.next_cursor, None);

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_aggregates_mcp_server_status_list_over_v2() {
        let worker_a = start_mock_remote_multi_connection_mcp_server_status_list_server(vec![
            "shared-mcp",
            "worker-a-mcp",
        ])
        .await;
        let worker_b = start_mock_remote_multi_connection_mcp_server_status_list_server(vec![
            "shared-mcp",
            "worker-b-mcp",
        ])
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_page: ListMcpServerStatusResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::McpServerStatusList {
                request_id: RequestId::Integer(1),
                params: ListMcpServerStatusParams {
                    cursor: None,
                    limit: Some(2),
                    detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                },
            }),
        )
        .await
        .expect("mcpServerStatus/list first page should finish in time")
        .expect("mcpServerStatus/list first page should aggregate through multi-worker gateway");
        assert_eq!(
            first_page
                .data
                .iter()
                .map(|status| status.name.as_str())
                .collect::<Vec<_>>(),
            vec!["shared-mcp", "worker-a-mcp"]
        );
        assert_eq!(
            first_page.next_cursor.as_deref(),
            Some("mcp-status-offset:2")
        );

        let second_page: ListMcpServerStatusResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::McpServerStatusList {
                request_id: RequestId::Integer(2),
                params: ListMcpServerStatusParams {
                    cursor: first_page.next_cursor.clone(),
                    limit: Some(2),
                    detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                },
            }),
        )
        .await
        .expect("mcpServerStatus/list second page should finish in time")
        .expect("mcpServerStatus/list second page should aggregate through multi-worker gateway");
        assert_eq!(
            second_page
                .data
                .iter()
                .map(|status| status.name.as_str())
                .collect::<Vec<_>>(),
            vec!["worker-b-mcp"]
        );
        assert_eq!(second_page.next_cursor, None);

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_deduplicates_skills_changed_notifications_over_v2() {
        let worker_a = start_mock_remote_multi_connection_skills_changed_server().await;
        let worker_b = start_mock_remote_multi_connection_skills_changed_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_notification = timeout(Duration::from_secs(5), client.next_event())
            .await
            .expect("initial skills/changed should finish in time")
            .expect("event stream should stay open");
        assert!(matches!(
            first_notification,
            AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
        ));
        assert!(
            timeout(Duration::from_millis(200), client.next_event())
                .await
                .is_err(),
            "duplicate initial skills/changed should be suppressed"
        );

        let _: SkillsListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::SkillsList {
                request_id: RequestId::Integer(1),
                params: SkillsListParams {
                    cwds: vec!["/tmp/shared-repo".into()],
                    force_reload: true,
                    per_cwd_extra_user_roots: None,
                },
            }),
        )
        .await
        .expect("skills/list should finish in time")
        .expect("skills/list should succeed through multi-worker gateway");

        let second_notification = timeout(Duration::from_secs(5), client.next_event())
            .await
            .expect("post-refresh skills/changed should finish in time")
            .expect("event stream should stay open");
        assert!(matches!(
            second_notification,
            AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
        ));
        assert!(
            timeout(Duration::from_millis(200), client.next_event())
                .await
                .is_err(),
            "duplicate post-refresh skills/changed should be suppressed"
        );

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_deduplicates_connection_state_notifications_over_v2() {
        let worker_a = start_mock_remote_multi_connection_state_notification_server().await;
        let worker_b = start_mock_remote_multi_connection_state_notification_server().await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let mut saw_account_updated = false;
        let mut saw_rate_limits = false;
        let mut saw_app_list = false;
        timeout(Duration::from_secs(5), async {
            while !(saw_account_updated && saw_rate_limits && saw_app_list) {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                        notification,
                    )) => {
                        assert_eq!(notification.auth_mode, None);
                        saw_account_updated = true;
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::AccountRateLimitsUpdated(notification),
                    ) => {
                        assert_eq!(notification.rate_limits.plan_type, None);
                        saw_rate_limits = true;
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AppListUpdated(
                        notification,
                    )) => {
                        assert_eq!(notification.data.len(), 1);
                        assert_eq!(notification.data[0].name, "calendar");
                        saw_app_list = true;
                    }
                    other => panic!("unexpected notification: {other:?}"),
                }
            }
        })
        .await
        .expect("deduplicated state notifications should arrive");
        assert!(saw_account_updated);
        assert!(saw_rate_limits);
        assert!(saw_app_list);
        assert!(
            timeout(Duration::from_millis(200), client.next_event())
                .await
                .is_err(),
            "duplicate connection-state notifications should be suppressed"
        );

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_supports_plugin_discovery_and_management_over_v2() {
        let worker_a = start_mock_remote_multi_plugin_server(
            "worker-a-marketplace",
            "/tmp/worker-a",
            "worker-a-plugin",
            "Worker A plugin detail",
        )
        .await;
        let worker_b = start_mock_remote_multi_plugin_server(
            "worker-b-marketplace",
            "/tmp/worker-b",
            "worker-b-plugin",
            "Worker B plugin detail",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let listed: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(1),
                params: serde_json::from_value(serde_json::json!({
                    "cwds": ["/tmp/shared-repo"],
                }))
                .expect("plugin/list params should deserialize"),
            })
            .await
            .expect("plugin/list should aggregate through multi-worker gateway");
        assert_eq!(listed.marketplaces.len(), 2);
        assert_eq!(
            listed
                .marketplaces
                .iter()
                .map(|marketplace| marketplace.name.as_str())
                .collect::<Vec<_>>(),
            vec!["worker-a-marketplace", "worker-b-marketplace"]
        );
        assert_eq!(
            listed.marketplaces[0].plugins[0].installed, false,
            "worker A plugin should start uninstalled"
        );
        assert_eq!(
            listed.marketplaces[1].plugins[0].installed, false,
            "worker B plugin should start uninstalled"
        );

        let plugin: PluginReadResponse = client
            .request_typed(ClientRequest::PluginRead {
                request_id: RequestId::Integer(2),
                params: serde_json::from_value(serde_json::json!({
                    "marketplacePath": "/tmp/worker-b/marketplace.json",
                    "pluginName": "worker-b-plugin",
                }))
                .expect("plugin/read params should deserialize"),
            })
            .await
            .expect("plugin/read should route to the matching worker");
        assert_eq!(plugin.plugin.marketplace_name, "worker-b-marketplace");
        assert_eq!(plugin.plugin.summary.name, "worker-b-plugin");
        assert_eq!(
            plugin.plugin.description.as_deref(),
            Some("Worker B plugin detail")
        );

        let install: PluginInstallResponse = client
            .request_typed(ClientRequest::PluginInstall {
                request_id: RequestId::Integer(3),
                params: serde_json::from_value(serde_json::json!({
                    "marketplacePath": "/tmp/worker-b/marketplace.json",
                    "pluginName": "worker-b-plugin",
                }))
                .expect("plugin/install params should deserialize"),
            })
            .await
            .expect("plugin/install should route to the matching worker");
        assert_eq!(install.auth_policy, PluginAuthPolicy::OnInstall);
        assert!(install.apps_needing_auth.is_empty());

        let listed_after_install: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(4),
                params: serde_json::from_value(serde_json::json!({
                    "cwds": ["/tmp/shared-repo"],
                }))
                .expect("plugin/list params after install should deserialize"),
            })
            .await
            .expect("plugin/list after install should aggregate through multi-worker gateway");
        assert_eq!(listed_after_install.marketplaces.len(), 2);
        assert_eq!(
            listed_after_install.marketplaces[0].plugins[0].installed,
            false
        );
        assert_eq!(
            listed_after_install.marketplaces[1].plugins[0].installed,
            true
        );

        let uninstall: PluginUninstallResponse = client
            .request_typed(ClientRequest::PluginUninstall {
                request_id: RequestId::Integer(5),
                params: PluginUninstallParams {
                    plugin_id: "worker-b-plugin@worker-b-marketplace".to_string(),
                },
            })
            .await
            .expect("plugin/uninstall should route to the installed worker");
        assert_eq!(uninstall, PluginUninstallResponse {});

        let listed_after_uninstall: PluginListResponse = client
            .request_typed(ClientRequest::PluginList {
                request_id: RequestId::Integer(6),
                params: serde_json::from_value(serde_json::json!({
                    "cwds": ["/tmp/shared-repo"],
                }))
                .expect("plugin/list params after uninstall should deserialize"),
            })
            .await
            .expect("plugin/list after uninstall should aggregate through multi-worker gateway");
        assert_eq!(listed_after_uninstall.marketplaces.len(), 2);
        assert_eq!(
            listed_after_uninstall.marketplaces[0].plugins[0].installed,
            false
        );
        assert_eq!(
            listed_after_uninstall.marketplaces[1].plugins[0].installed,
            false
        );

        assert_remote_client_shutdown(
            timeout(Duration::from_secs(5), client.shutdown())
                .await
                .expect("client shutdown should finish in time"),
        );
        timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown should finish in time")
            .expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_supports_v2_turn_routing_and_notification_fan_in() {
        let worker_a = start_mock_remote_multi_connection_workflow_server(
            "thread-worker-a",
            "/tmp/worker-a",
            "turn-thread-worker-a",
            "hello from worker a",
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_workflow_server(
            "thread-worker-b",
            "/tmp/worker-b",
            "turn-thread-worker-b",
            "hello from worker b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 16,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");
        let mut client = client;

        let first_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed through multi-worker remote gateway");

        let first_turn_started: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(3),
                params: TurnStartParams {
                    thread_id: first_started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "hello worker a".to_string(),
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
                },
            })
            .await
            .expect("first turn/start should route to worker A");
        assert_eq!(first_turn_started.turn.id, "turn-thread-worker-a");
        assert_eq!(first_turn_started.turn.status, TurnStatus::InProgress);

        let second_turn_started: TurnStartResponse = client
            .request_typed(ClientRequest::TurnStart {
                request_id: RequestId::Integer(4),
                params: TurnStartParams {
                    thread_id: second_started.thread.id.clone(),
                    input: vec![UserInput::Text {
                        text: "hello worker b".to_string(),
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
                },
            })
            .await
            .expect("second turn/start should route to worker B");
        assert_eq!(second_turn_started.turn.id, "turn-thread-worker-b");
        assert_eq!(second_turn_started.turn.status, TurnStatus::InProgress);

        let mut lifecycle_by_thread = HashMap::from([
            (
                first_started.thread.id.clone(),
                TurnStreamingCoverage::default(),
            ),
            (
                second_started.thread.id.clone(),
                TurnStreamingCoverage::default(),
            ),
        ]);
        let expected_turn_by_thread = HashMap::from([
            (
                first_started.thread.id.clone(),
                ("turn-thread-worker-a", "hello from worker a"),
            ),
            (
                second_started.thread.id.clone(),
                ("turn-thread-worker-b", "hello from worker b"),
            ),
        ]);

        let turn_fan_in_result = timeout(Duration::from_secs(5), async {
            while lifecycle_by_thread.values().any(|coverage| {
                *coverage
                    != (TurnStreamingCoverage {
                        saw_thread_active: true,
                        saw_turn_started: true,
                        saw_agent_delta: true,
                        saw_reasoning_summary_delta: true,
                        saw_reasoning_text_delta: true,
                        saw_command_output_delta: true,
                        saw_file_change_delta: true,
                        saw_turn_completed: true,
                    })
            }) {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerNotification(
                        ServerNotification::ThreadStatusChanged(notification),
                    ) => {
                        if let Some(coverage) = lifecycle_by_thread.get_mut(&notification.thread_id)
                            && matches!(
                                notification.status,
                                ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                            )
                        {
                            coverage.saw_thread_active = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                        notification,
                    )) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn.id == *expected_turn_id
                        {
                            coverage.saw_turn_started = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                        notification,
                    )) => {
                        if let Some((expected_turn_id, expected_delta)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn_id == *expected_turn_id
                            && notification.delta == *expected_delta
                        {
                            coverage.saw_agent_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ReasoningSummaryTextDelta(notification),
                    ) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn_id == *expected_turn_id
                            && notification.delta == format!("summary {}", notification.thread_id)
                            && notification.summary_index == 0
                        {
                            coverage.saw_reasoning_summary_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                        notification,
                    )) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn_id == *expected_turn_id
                            && notification.delta == format!("reasoning {}", notification.thread_id)
                            && notification.content_index == 0
                        {
                            coverage.saw_reasoning_text_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::CommandExecutionOutputDelta(notification),
                    ) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn_id == *expected_turn_id
                            && notification.delta == format!("stdout {}", notification.thread_id)
                        {
                            coverage.saw_command_output_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::FileChangeOutputDelta(notification),
                    ) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn_id == *expected_turn_id
                            && notification.delta == format!("patch {}", notification.thread_id)
                        {
                            coverage.saw_file_change_delta = true;
                        }
                    }
                    AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                        notification,
                    )) => {
                        if let Some((expected_turn_id, _)) =
                            expected_turn_by_thread.get(&notification.thread_id)
                            && let Some(coverage) =
                                lifecycle_by_thread.get_mut(&notification.thread_id)
                            && notification.turn.id == *expected_turn_id
                            && notification.turn.status == TurnStatus::Completed
                        {
                            coverage.saw_turn_completed = true;
                        }
                    }
                    _ => {}
                }
            }
        })
        .await;
        assert_eq!(
            turn_fan_in_result.is_ok(),
            true,
            "turn notifications should fan in from both workers: {lifecycle_by_thread:?}"
        );

        assert_eq!(
            lifecycle_by_thread.get(&first_started.thread.id),
            Some(&TurnStreamingCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_agent_delta: true,
                saw_reasoning_summary_delta: true,
                saw_reasoning_text_delta: true,
                saw_command_output_delta: true,
                saw_file_change_delta: true,
                saw_turn_completed: true,
            })
        );
        assert_eq!(
            lifecycle_by_thread.get(&second_started.thread.id),
            Some(&TurnStreamingCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_agent_delta: true,
                saw_reasoning_summary_delta: true,
                saw_reasoning_text_delta: true,
                saw_command_output_delta: true,
                saw_file_change_delta: true,
                saw_turn_completed: true,
            })
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_supports_v2_server_request_routing_and_id_translation() {
        let worker_a = start_mock_remote_multi_connection_server_request_server(
            "thread-worker-a",
            "/tmp/worker-a",
            "safe-a",
            "acct-worker-a",
        )
        .await;
        let worker_b = start_mock_remote_multi_connection_server_request_server(
            "thread-worker-b",
            "/tmp/worker-b",
            "safe-b",
            "acct-worker-b",
        )
        .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
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
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed through multi-worker remote gateway");

        let expected_threads = HashSet::from([
            first_started.thread.id.clone(),
            second_started.thread.id.clone(),
        ]);
        let mut gateway_request_ids = HashSet::new();
        let mut user_input_threads = HashSet::new();
        let mut command_threads = HashSet::new();
        let mut file_threads = HashSet::new();
        let mut permissions_threads = HashSet::new();
        let mut mcp_threads = HashSet::new();
        let mut refresh_accounts = HashSet::new();
        timeout(Duration::from_secs(5), async {
            while user_input_threads.len() < expected_threads.len()
                || command_threads.len() < expected_threads.len()
                || file_threads.len() < expected_threads.len()
                || permissions_threads.len() < expected_threads.len()
                || mcp_threads.len() < expected_threads.len()
                || refresh_accounts.len() < expected_threads.len()
            {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.item_id, "tool-call-remote-workflow");
                        assert_eq!(params.questions.len(), 1);
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-server-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        let answer = match params.thread_id.as_str() {
                            "thread-worker-a" => "safe-a",
                            "thread-worker-b" => "safe-b",
                            thread_id => panic!("unexpected thread id: {thread_id}"),
                        };
                        let mut answers = HashMap::new();
                        answers.insert(
                            "mode".to_string(),
                            ToolRequestUserInputAnswer {
                                answers: vec![answer.to_string()],
                            },
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(ToolRequestUserInputResponse { answers })
                                    .expect("server request response should serialize"),
                            )
                            .await
                            .expect("server request should resolve");
                        user_input_threads.insert(params.thread_id);
                    }
                    AppServerEvent::ServerRequest(
                        ServerRequest::CommandExecutionRequestApproval { request_id, params },
                    ) => {
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "cmd-remote-workflow");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-command-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(CommandExecutionRequestApprovalResponse {
                                    decision: CommandExecutionApprovalDecision::Accept,
                                })
                                .expect("command approval response should serialize"),
                            )
                            .await
                            .expect("command approval should resolve");
                        command_threads.insert(params.thread_id);
                    }
                    AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "file-remote-workflow");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-file-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(FileChangeRequestApprovalResponse {
                                    decision: FileChangeApprovalDecision::Accept,
                                })
                                .expect("file approval response should serialize"),
                            )
                            .await
                            .expect("file approval should resolve");
                        file_threads.insert(params.thread_id);
                    }
                    AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "perm-remote-workflow");
                        assert_eq!(params.reason, Some("Need wider permissions".to_string()));
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-permissions-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(PermissionsRequestApprovalResponse {
                                    permissions:
                                        codex_app_server_protocol::GrantedPermissionProfile {
                                            network: params.permissions.network,
                                            file_system: params.permissions.file_system,
                                        },
                                    scope: PermissionGrantScope::Turn,
                                })
                                .expect("permissions approval response should serialize"),
                            )
                            .await
                            .expect("permissions approval should resolve");
                        permissions_threads.insert(params.thread_id);
                    }
                    AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                        assert_eq!(params.server_name, "mock-mcp");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-mcp-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(McpServerElicitationRequestResponse {
                                    action: McpServerElicitationAction::Accept,
                                    content: Some(serde_json::json!({
                                        "confirmed": true,
                                    })),
                                    meta: None,
                                })
                                .expect("mcp elicitation response should serialize"),
                            )
                            .await
                            .expect("mcp elicitation should resolve");
                        mcp_threads.insert(params.thread_id);
                    }
                    AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                        request_id,
                        params,
                    }) => {
                        let previous_account_id = params
                            .previous_account_id
                            .as_deref()
                            .expect("refresh request should include previous account id");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-chatgpt-refresh-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should be unique across workers and methods"
                        );
                        client
                            .resolve_server_request(
                                request_id,
                                serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                    access_token: format!("access-token-{previous_account_id}"),
                                    chatgpt_account_id: previous_account_id.to_string(),
                                    chatgpt_plan_type: Some("pro".to_string()),
                                })
                                .expect("chatgpt refresh response should serialize"),
                            )
                            .await
                            .expect("chatgpt refresh should resolve");
                        refresh_accounts.insert(previous_account_id.to_string());
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("server requests should arrive from both workers");
        assert_eq!(user_input_threads, expected_threads);
        assert_eq!(command_threads, expected_threads);
        assert_eq!(file_threads, expected_threads);
        assert_eq!(permissions_threads, expected_threads);
        assert_eq!(mcp_threads, expected_threads);
        assert_eq!(
            refresh_accounts,
            HashSet::from(["acct-worker-a".to_string(), "acct-worker-b".to_string(),])
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_multi_worker_routes_thread_mutations_to_owning_workers() {
        let worker_a =
            start_mock_remote_multi_connection_mutation_server("thread-worker-a", "/tmp/worker-a")
                .await;
        let worker_b =
            start_mock_remote_multi_connection_mutation_server("thread-worker-b", "/tmp/worker-b")
                .await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_a,
                            auth_token: None,
                        },
                        GatewayRemoteWorkerConfig {
                            websocket_url: worker_b,
                            auth_token: None,
                        },
                    ],
                }),
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await
        .expect("server");

        let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
            client_name: "codex-gateway-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        })
        .await
        .expect("remote client should connect to multi-worker gateway");

        let first_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(1),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-a".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("first thread/start should succeed through multi-worker remote gateway");
        let second_started: AppServerThreadStartResponse = client
            .request_typed(ClientRequest::ThreadStart {
                request_id: RequestId::Integer(2),
                params: ThreadStartParams {
                    model: None,
                    model_provider: None,
                    service_tier: None,
                    cwd: Some("/tmp/project-b".to_string()),
                    approval_policy: None,
                    approvals_reviewer: None,
                    sandbox: None,
                    config: None,
                    service_name: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                    ephemeral: Some(true),
                    session_start_source: None,
                    dynamic_tools: None,
                    mock_experimental_field: None,
                    experimental_raw_events: false,
                    persist_extended_history: false,
                },
            })
            .await
            .expect("second thread/start should succeed through multi-worker remote gateway");

        let first_name = "Worker A Thread".to_string();
        let second_name = "Worker B Thread".to_string();

        let first_rename: ThreadSetNameResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadSetName {
                request_id: RequestId::Integer(3),
                params: ThreadSetNameParams {
                    thread_id: first_started.thread.id.clone(),
                    name: first_name.clone(),
                },
            }),
        )
        .await
        .expect("first thread/name/set should finish in time")
        .expect("first thread/name/set should route to worker A");
        assert_eq!(first_rename, ThreadSetNameResponse {});

        let second_rename: ThreadSetNameResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadSetName {
                request_id: RequestId::Integer(4),
                params: ThreadSetNameParams {
                    thread_id: second_started.thread.id.clone(),
                    name: second_name.clone(),
                },
            }),
        )
        .await
        .expect("second thread/name/set should finish in time")
        .expect("second thread/name/set should route to worker B");
        assert_eq!(second_rename, ThreadSetNameResponse {});

        let first_read: AppServerThreadReadResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(5),
                params: ThreadReadParams {
                    thread_id: first_started.thread.id.clone(),
                    include_turns: false,
                },
            }),
        )
        .await
        .expect("first thread/read should finish in time")
        .expect("thread/read should route renamed thread back to worker A");
        assert_eq!(first_read.thread.id, first_started.thread.id);
        assert_eq!(first_read.thread.name, Some(first_name.clone()));
        assert_eq!(
            first_read.thread.cwd.as_ref().to_string_lossy(),
            "/tmp/worker-a"
        );

        let second_read: AppServerThreadReadResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadRead {
                request_id: RequestId::Integer(6),
                params: ThreadReadParams {
                    thread_id: second_started.thread.id.clone(),
                    include_turns: false,
                },
            }),
        )
        .await
        .expect("second thread/read should finish in time")
        .expect("thread/read should route renamed thread back to worker B");
        assert_eq!(second_read.thread.id, second_started.thread.id);
        assert_eq!(second_read.thread.name, Some(second_name.clone()));
        assert_eq!(
            second_read.thread.cwd.as_ref().to_string_lossy(),
            "/tmp/worker-b"
        );

        let listed: AppServerThreadListResponse = timeout(
            Duration::from_secs(5),
            client.request_typed(ClientRequest::ThreadList {
                request_id: RequestId::Integer(7),
                params: ThreadListParams {
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
            }),
        )
        .await
        .expect("thread/list should finish in time")
        .expect("thread/list should aggregate renamed threads");
        let names_by_thread = listed
            .data
            .into_iter()
            .map(|thread| (thread.id, thread.name))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            names_by_thread.get(&first_started.thread.id),
            Some(&Some(first_name))
        );
        assert_eq!(
            names_by_thread.get(&second_started.thread.id),
            Some(&Some(second_name))
        );

        assert_remote_client_shutdown(client.shutdown().await);
        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_reconnects_workers_after_disconnect() {
        let worker_a =
            start_reconnecting_mock_remote_server(Some("secret-token".to_string())).await;
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: Some(GatewayRemoteRuntimeConfig {
                    selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                    workers: vec![GatewayRemoteWorkerConfig {
                        websocket_url: worker_a.clone(),
                        auth_token: Some("secret-token".to_string()),
                    }],
                }),
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
            .json(&CreateThreadRequest {
                cwd: Some("/tmp/project-a".to_string()),
                model: None,
                ephemeral: Some(true),
            })
            .send()
            .await
            .expect("first response");
        assert_eq!(first_response.status(), reqwest::StatusCode::OK);
        let first_thread: ThreadResponse = first_response.json().await.expect("first thread");
        assert_eq!(first_thread.thread.id, "thread-worker-a-1");

        let second_response = timeout(Duration::from_secs(5), async {
            loop {
                sleep(Duration::from_millis(100)).await;
                let response = client
                    .post(format!("http://{}/v1/threads", server.local_addr()))
                    .json(&CreateThreadRequest {
                        cwd: Some("/tmp/project-b".to_string()),
                        model: None,
                        ephemeral: Some(true),
                    })
                    .send()
                    .await
                    .expect("second response");
                if response.status() == reqwest::StatusCode::OK {
                    break response;
                }
            }
        })
        .await
        .expect("worker should reconnect");
        let second_thread: ThreadResponse = second_response.json().await.expect("second thread");
        assert_eq!(second_thread.thread.id, "thread-worker-a-2");

        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.status, GatewayHealthStatus::Ok);
        let remote_workers = health.remote_workers.expect("remote workers");
        assert_eq!(remote_workers.len(), 1);
        assert_eq!(remote_workers[0].websocket_url, worker_a);
        assert_eq!(remote_workers[0].healthy, true);
        assert_eq!(
            remote_workers[0]
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("remote app server")),
            true
        );

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn embedded_runtime_healthz_reports_exec_server_execution_mode() {
        let exec_server_url = start_mock_exec_server().await;
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
                exec_server_url: Some(exec_server_url),
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
        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.status, GatewayHealthStatus::Ok);
        assert_eq!(health.runtime_mode, "embedded");
        assert_eq!(health.execution_mode, GatewayExecutionMode::ExecServer);
        assert_eq!(health.v2_transport.initialize_timeout_seconds, 30);
        assert_eq!(health.v2_transport.client_send_timeout_seconds, 10);
        assert_eq!(health.v2_transport.max_pending_server_requests, 64);
        assert_eq!(health.remote_workers, None);

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn healthz_reports_configured_v2_transport_settings() {
        let codex_home = tempdir().expect("tempdir");
        let config = Config::load_default_with_cli_overrides_for_codex_home(
            codex_home.path().to_path_buf(),
            Vec::new(),
        )
        .await
        .expect("config");
        let server = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                v2_initialize_timeout: Duration::from_secs(7),
                v2_client_send_timeout: Duration::from_secs(3),
                v2_max_pending_server_requests: 11,
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
        let healthz_response = client
            .get(format!("http://{}/healthz", server.local_addr()))
            .send()
            .await
            .expect("healthz response");
        assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
        let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
        assert_eq!(health.v2_transport.initialize_timeout_seconds, 7);
        assert_eq!(health.v2_transport.client_send_timeout_seconds, 3);
        assert_eq!(health.v2_transport.max_pending_server_requests, 11);

        server.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn remote_runtime_requires_websocket_configuration() {
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");
        let result = start_gateway_server(
            GatewayConfig {
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                runtime_mode: GatewayRuntimeMode::Remote,
                remote_runtime: None,
                ..GatewayConfig::default()
            },
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await;

        assert_eq!(result.is_err(), true);
        assert_eq!(
            result.err().expect("expected missing remote config").kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    async fn start_mock_remote_server(
        expected_auth_token: Option<String>,
        thread_id: &'static str,
        preview: &'static str,
    ) -> String {
        start_mock_remote_server_with_options(MockRemoteServerOptions {
            expected_auth_token,
            thread_id,
            preview,
            close_after_first_request: false,
        })
        .await
    }

    async fn start_reconnecting_mock_remote_server(expected_auth_token: Option<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            for connection_index in 0..2 {
                let expected_auth_token = expected_auth_token.clone();
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let mut websocket = accept_hdr_async(
                    stream,
                    move |request: &WebSocketRequest, response: WebSocketResponse| {
                        let provided_auth_token = request
                            .headers()
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_owned);
                        let expected_header = expected_auth_token
                            .as_ref()
                            .map(|token| format!("Bearer {token}"));
                        assert_eq!(provided_auth_token, expected_header);
                        Ok(response)
                    },
                )
                .await
                .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                loop {
                    let message = read_websocket_message(&mut websocket).await;
                    let JSONRPCMessage::Request(request) = message else {
                        panic!("expected request");
                    };
                    assert_eq!(request.method, "thread/start");
                    let thread_id = if connection_index == 0 {
                        "thread-worker-a-1"
                    } else {
                        "thread-worker-a-2"
                    };
                    let preview = if connection_index == 0 {
                        "/tmp/worker-a-1"
                    } else {
                        "/tmp/worker-a-2"
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({
                                "thread": mock_thread(thread_id, preview),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;
                    if connection_index == 0 {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                        break;
                    }
                }
            }
        });
        format!("ws://{addr}")
    }

    async fn start_reconnecting_v2_mock_remote_server(
        expected_auth_token: Option<String>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            for connection_index in 0..3 {
                let expected_auth_token = expected_auth_token.clone();
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let mut websocket = accept_hdr_async(
                    stream,
                    move |request: &WebSocketRequest, response: WebSocketResponse| {
                        let provided_auth_token = request
                            .headers()
                            .get(AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_owned);
                        let expected_header = expected_auth_token
                            .as_ref()
                            .map(|token| format!("Bearer {token}"));
                        assert_eq!(provided_auth_token, expected_header);
                        Ok(response)
                    },
                )
                .await
                .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                            drop(websocket);
                        });
                    }
                    2 => {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        assert_eq!(request.method, "thread/start");
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result: serde_json::json!({
                                    "thread": mock_thread("thread-worker-a-2", "/tmp/worker-a-2"),
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": "/tmp/worker-a-2",
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess"
                                    },
                                    "reasoningEffort": null,
                                }),
                            }),
                        )
                        .await;

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                                id: RequestId::String("srv-user-input-after-reconnect".to_string()),
                                method: "item/tool/requestUserInput".to_string(),
                                params: Some(
                                    serde_json::to_value(
                                        codex_app_server_protocol::ToolRequestUserInputParams {
                                            thread_id: "thread-worker-a-2".to_string(),
                                            turn_id: "turn-worker-a-2".to_string(),
                                            item_id: "tool-call-worker-a-2".to_string(),
                                            questions: vec![ToolRequestUserInputQuestion {
                                                id: "mode".to_string(),
                                                header: "Mode".to_string(),
                                                question: "Pick execution mode".to_string(),
                                                is_other: false,
                                                is_secret: false,
                                                options: Some(vec![]),
                                            }],
                                        },
                                    )
                                    .expect("server request params should serialize"),
                                ),
                                trace: None,
                            }),
                        )
                        .await;

                        let JSONRPCMessage::Response(response) =
                            read_websocket_message(&mut websocket).await
                        else {
                            panic!("expected server request response");
                        };
                        assert_eq!(
                            response.id,
                            RequestId::String("srv-user-input-after-reconnect".to_string())
                        );
                        assert_eq!(
                            response.result,
                            serde_json::json!({
                                "answers": {
                                    "mode": {
                                        "answers": ["safe"],
                                    },
                                },
                            })
                        );
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            }
        });
        format!("ws://{addr}")
    }

    async fn start_reconnecting_v2_multi_connection_thread_server(
        thread_id: &'static str,
        preview: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            for connection_index in 0..3 {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                            drop(websocket);
                        });
                    }
                    2 => loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        match request.method.as_str() {
                            "thread/start" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": mock_thread(thread_id, preview),
                                            "model": "gpt-5",
                                            "modelProvider": "openai",
                                            "serviceTier": null,
                                            "cwd": preview,
                                            "instructionSources": [],
                                            "approvalPolicy": "never",
                                            "approvalsReviewer": "user",
                                            "sandbox": {
                                                "type": "dangerFullAccess"
                                            },
                                            "reasoningEffort": null,
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Request(
                                        codex_app_server_protocol::JSONRPCRequest {
                                            id: RequestId::String(
                                                "srv-user-input-after-reconnect".to_string(),
                                            ),
                                            method: "item/tool/requestUserInput".to_string(),
                                            params: Some(
                                                serde_json::to_value(
                                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                                        thread_id: thread_id.to_string(),
                                                        turn_id: "turn-worker-a-2".to_string(),
                                                        item_id: "tool-call-worker-a-2".to_string(),
                                                        questions: vec![ToolRequestUserInputQuestion {
                                                            id: "mode".to_string(),
                                                            header: "Mode".to_string(),
                                                            question: "Pick execution mode".to_string(),
                                                            is_other: false,
                                                            is_secret: false,
                                                            options: Some(vec![]),
                                                        }],
                                                    },
                                                )
                                                .expect("server request params should serialize"),
                                            ),
                                            trace: None,
                                        },
                                    ),
                                )
                                .await;
                                let JSONRPCMessage::Response(response) =
                                    read_websocket_message(&mut websocket).await
                                else {
                                    panic!("expected server request response");
                                };
                                assert_eq!(
                                    response.id,
                                    RequestId::String("srv-user-input-after-reconnect".to_string())
                                );
                                assert_eq!(
                                    response.result,
                                    serde_json::json!({
                                        "answers": {
                                            "mode": {
                                                "answers": ["safe"],
                                            },
                                        },
                                    })
                                );
                            }
                            "thread/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": mock_thread(thread_id, preview),
                                            "turns": [],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [mock_thread(thread_id, preview)],
                                            "nextCursor": null,
                                            "backwardsCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/loaded/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [thread_id],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "turn/start" => {
                                let requested_thread_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("threadId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("turn/start should include threadId");
                                assert_eq!(requested_thread_id, thread_id);
                                let turn_id = format!("turn-{thread_id}");

                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "turn": mock_turn(&turn_id, "inProgress"),
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "thread/status/changed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "status": {
                                                    "type": "active",
                                                    "activeFlags": [],
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/started".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(&turn_id, "inProgress"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/agentMessage/delta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("msg-{thread_id}"),
                                                "delta": "hello from recovered worker",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-summary-{thread_id}"),
                                                "delta": format!("summary {thread_id}"),
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-{thread_id}"),
                                                "delta": format!("reasoning {thread_id}"),
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("exec-{thread_id}"),
                                                "delta": format!("stdout {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("patch-{thread_id}"),
                                                "delta": format!("patch {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(&turn_id, "completed"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            method => panic!("unexpected request method: {method}"),
                        }
                    },
                    _ => unreachable!("unexpected connection index"),
                }
            }
        });
        format!("ws://{addr}")
    }

    async fn start_reconnecting_v2_multi_connection_state_notification_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            for connection_index in 0..3 {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                            drop(websocket);
                        });
                    }
                    2 => {
                        for notification in [
                            serde_json::json!({
                                "method": "account/updated",
                                "params": {
                                    "authMode": null,
                                    "planType": null,
                                },
                            }),
                            serde_json::json!({
                                "method": "account/rateLimits/updated",
                                "params": {
                                    "rateLimits": {
                                        "limitId": null,
                                        "limitName": null,
                                        "primary": null,
                                        "secondary": null,
                                        "credits": null,
                                        "planType": null,
                                        "rateLimitReachedType": null,
                                    },
                                },
                            }),
                            serde_json::json!({
                                "method": "app/list/updated",
                                "params": {
                                    "data": [{
                                        "id": "calendar",
                                        "name": "calendar",
                                        "description": null,
                                        "logoUrl": null,
                                        "logoUrlDark": null,
                                        "distributionChannel": null,
                                        "branding": null,
                                        "appMetadata": null,
                                        "labels": null,
                                        "installUrl": null,
                                    }],
                                },
                            }),
                        ] {
                            write_websocket_message(
                                &mut websocket,
                                serde_json::from_value::<JSONRPCMessage>(notification)
                                    .expect("notification should decode"),
                            )
                            .await;
                        }

                        while let Some(frame) = websocket.next().await {
                            match frame.expect("follow-up frame should decode") {
                                Message::Close(_) => break,
                                Message::Ping(payload) => {
                                    websocket
                                        .send(Message::Pong(payload))
                                        .await
                                        .expect("pong should send");
                                }
                                Message::Text(_)
                                | Message::Binary(_)
                                | Message::Pong(_)
                                | Message::Frame(_) => {}
                            }
                        }
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            }
        });
        format!("ws://{addr}")
    }

    async fn start_reconnecting_v2_multi_connection_skills_changed_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            for connection_index in 0..3 {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;
                match connection_index {
                    0 => {
                        websocket
                            .close(None)
                            .await
                            .expect("close frame should send");
                    }
                    1 => {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(60)).await;
                            drop(websocket);
                        });
                    }
                    2 => {
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Notification(
                                codex_app_server_protocol::JSONRPCNotification {
                                    method: "skills/changed".to_string(),
                                    params: Some(serde_json::json!({})),
                                },
                            ),
                        )
                        .await;

                        loop {
                            let message = read_websocket_message(&mut websocket).await;
                            let JSONRPCMessage::Request(request) = message else {
                                panic!("expected request");
                            };
                            match request.method.as_str() {
                                "skills/list" => {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id,
                                            result: serde_json::json!({
                                                "data": [{
                                                    "cwd": "/tmp/shared-repo",
                                                    "skills": [],
                                                    "errors": [],
                                                }],
                                            }),
                                        }),
                                    )
                                    .await;
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "skills/changed".to_string(),
                                                params: Some(serde_json::json!({})),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                method => panic!("unexpected request method: {method}"),
                            }
                        }
                    }
                    _ => unreachable!("unexpected connection index"),
                }
            }
        });
        format!("ws://{addr}")
    }

    fn write_embedded_plugin_fixture(codex_home: &std::path::Path) {
        let plugin_root = codex_home.join("plugins/demo-plugin");
        std::fs::create_dir_all(codex_home.join(".git")).expect(".git should be created");
        std::fs::create_dir_all(codex_home.join(".agents/plugins"))
            .expect("marketplace dir should be created");
        std::fs::create_dir_all(plugin_root.join(".codex-plugin"))
            .expect("plugin manifest dir should be created");
        std::fs::write(
            codex_home.join(".agents/plugins/marketplace.json"),
            r#"{
  "name": "local",
  "plugins": [
    {
      "name": "demo-plugin",
      "source": {
        "source": "local",
        "path": "./plugins/demo-plugin"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      }
    }
  ]
}"#,
        )
        .expect("marketplace.json should be written");
        std::fs::write(
            plugin_root.join(".codex-plugin/plugin.json"),
            r##"{
  "name": "demo-plugin",
  "description": "Demo plugin detail",
  "interface": {
    "displayName": "Demo Plugin",
    "shortDescription": "Demo plugin short description",
    "longDescription": "Demo plugin long description",
    "developerName": "OpenAI",
    "category": "tools",
    "capabilities": ["commands"],
    "websiteURL": "https://openai.com/",
    "privacyPolicyURL": null,
    "termsOfServiceURL": null,
    "defaultPrompt": null,
    "brandColor": null,
    "composerIcon": null,
    "logo": null,
    "screenshots": []
  }
}"##,
        )
        .expect("plugin.json should be written");
    }

    async fn start_mock_remote_workflow_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");

                    expect_remote_initialize(&mut websocket).await;

                    let thread_id = "thread-remote-workflow";
                    let preview = "/tmp/remote-project";
                    let mut thread_name: Option<String> = None;
                    let mut plugin_installed = false;
                    let turn_id = "turn-remote-workflow";

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "account/read" => serde_json::json!({
                                "account": null,
                                "requiresOpenaiAuth": false,
                            }),
                            "model/list" => serde_json::json!({
                                "data": [{
                                    "id": "openai/gpt-5",
                                    "model": "gpt-5",
                                    "upgrade": null,
                                    "upgradeInfo": null,
                                    "availabilityNux": null,
                                    "displayName": "GPT-5",
                                    "description": "Remote gateway workflow model",
                                    "hidden": false,
                                    "supportedReasoningEfforts": [{
                                        "reasoningEffort": "medium",
                                        "description": "Balanced"
                                    }],
                                    "defaultReasoningEffort": "medium",
                                    "inputModalities": ["text"],
                                    "supportsPersonality": false,
                                    "additionalSpeedTiers": [],
                                    "isDefault": true
                                }],
                                "nextCursor": null,
                            }),
                            "externalAgentConfig/detect" => serde_json::json!({
                                "items": [],
                            }),
                            "externalAgentConfig/import" => serde_json::json!({}),
                            "skills/list" => serde_json::json!({
                                "data": [{
                                    "cwd": preview,
                                    "skills": [],
                                    "errors": [],
                                }],
                            }),
                            "plugin/list" => serde_json::json!({
                                "marketplaces": [{
                                    "name": "remote-marketplace",
                                    "path": format!("{preview}/marketplace.json"),
                                    "interface": {
                                        "displayName": "Remote Marketplace",
                                    },
                                    "plugins": [{
                                        "id": "remote-plugin",
                                        "name": "remote-plugin",
                                        "source": {
                                            "type": "local",
                                            "path": format!("{preview}/plugins/remote-plugin"),
                                        },
                                        "installed": plugin_installed,
                                        "enabled": plugin_installed,
                                        "installPolicy": "AVAILABLE",
                                        "authPolicy": "ON_INSTALL",
                                        "interface": {
                                            "displayName": "Remote Plugin",
                                            "shortDescription": "Remote plugin short description",
                                            "longDescription": null,
                                            "developerName": null,
                                            "category": "tools",
                                            "capabilities": ["commands"],
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
                                "featuredPluginIds": ["remote-plugin"],
                            }),
                            "plugin/read" => serde_json::json!({
                                "plugin": {
                                    "marketplaceName": "remote-marketplace",
                                    "marketplacePath": format!("{preview}/marketplace.json"),
                                    "summary": {
                                        "id": "remote-plugin",
                                        "name": "remote-plugin",
                                        "source": {
                                            "type": "local",
                                            "path": format!("{preview}/plugins/remote-plugin"),
                                        },
                                        "installed": plugin_installed,
                                        "enabled": plugin_installed,
                                        "installPolicy": "AVAILABLE",
                                        "authPolicy": "ON_INSTALL",
                                        "interface": {
                                            "displayName": "Remote Plugin",
                                            "shortDescription": "Remote plugin short description",
                                            "longDescription": null,
                                            "developerName": null,
                                            "category": "tools",
                                            "capabilities": ["commands"],
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
                                    },
                                    "description": "Remote plugin detail",
                                    "skills": [{
                                        "name": "remote-skill",
                                        "description": "Remote skill description",
                                        "shortDescription": "Remote skill short description",
                                        "interface": null,
                                        "path": format!("{preview}/skills/remote-skill"),
                                        "enabled": true,
                                    }],
                                    "apps": [],
                                    "mcpServers": ["remote-mcp"],
                                },
                            }),
                            "plugin/install" => {
                                plugin_installed = true;
                                serde_json::json!({
                                    "authPolicy": "ON_INSTALL",
                                    "appsNeedingAuth": [],
                                })
                            }
                            "plugin/uninstall" => {
                                plugin_installed = false;
                                serde_json::json!({})
                            }
                            "config/value/write" => serde_json::json!({
                                "status": "ok",
                                "version": "remote-version-1",
                                "filePath": format!("{preview}/config.toml"),
                                "overriddenMetadata": null,
                            }),
                            "config/batchWrite" => serde_json::json!({
                                "status": "ok",
                                "version": "remote-version-1",
                                "filePath": format!("{preview}/config.toml"),
                                "overriddenMetadata": null,
                            }),
                            "memory/reset" | "account/logout" => serde_json::json!({}),
                            "thread/start" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                            "model": "gpt-5",
                                            "modelProvider": "openai",
                                            "serviceTier": null,
                                            "cwd": preview,
                                            "instructionSources": [],
                                            "approvalPolicy": "never",
                                            "approvalsReviewer": "user",
                                            "sandbox": {
                                                "type": "dangerFullAccess"
                                            },
                                            "reasoningEffort": null,
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                        })),
                                    }),
                                )
                                .await;
                                continue;
                            }
                            "thread/list" => serde_json::json!({
                                "data": [mock_thread_with_name(thread_id, preview, thread_name.as_deref())],
                                "nextCursor": null,
                                "backwardsCursor": null,
                            }),
                            "thread/loaded/list" => serde_json::json!({
                                "data": [thread_id],
                                "nextCursor": null,
                            }),
                            "thread/read" => serde_json::json!({
                                "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                            "thread/name/set" => {
                                let name = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("name"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("thread/name/set should include name");
                                thread_name = Some(name.to_string());
                                serde_json::json!({})
                            }
                            "thread/memoryMode/set" => serde_json::json!({}),
                            "turn/start" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "turn": mock_turn(turn_id, "inProgress"),
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "thread/status/changed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "status": {
                                                    "type": "active",
                                                    "activeFlags": [],
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": "reasoning-summary-remote-workflow",
                                                "delta": "remote summary",
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": "reasoning-remote-workflow",
                                                "delta": "remote reasoning",
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": "exec-remote-workflow",
                                                "delta": "remote stdout",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": "patch-remote-workflow",
                                                "delta": "remote patch",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/started".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(turn_id, "inProgress"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/agentMessage/delta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": "msg-remote-workflow",
                                                "delta": "hello back",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-summary-{thread_id}"),
                                                "delta": format!("summary {thread_id}"),
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-{thread_id}"),
                                                "delta": format!("reasoning {thread_id}"),
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("exec-{thread_id}"),
                                                "delta": format!("stdout {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("patch-{thread_id}"),
                                                "delta": format!("patch {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-summary-{thread_id}"),
                                                "delta": format!("summary {thread_id}"),
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-{thread_id}"),
                                                "delta": format!("reasoning {thread_id}"),
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("exec-{thread_id}"),
                                                "delta": format!("stdout {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("patch-{thread_id}"),
                                                "delta": format!("patch {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-summary-{thread_id}"),
                                                "delta": format!("summary {thread_id}"),
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-{thread_id}"),
                                                "delta": format!("reasoning {thread_id}"),
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("exec-{thread_id}"),
                                                "delta": format!("stdout {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("patch-{thread_id}"),
                                                "delta": format!("patch {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(turn_id, "completed"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "thread/status/changed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "status": {
                                                    "type": "idle",
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                continue;
                            }
                            "thread/resume" | "thread/fork" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        id: request.id,
                                        error: JSONRPCErrorError {
                                            code: -32000,
                                            message: format!(
                                                "no rollout found for thread id {thread_id}"
                                            ),
                                            data: None,
                                        },
                                    }),
                                )
                                .await;
                                continue;
                            }
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_thread_server(
        thread_id: &'static str,
        preview: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "thread/start" => serde_json::json!({
                                "thread": mock_thread(thread_id, preview),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                            "thread/list" => serde_json::json!({
                                "data": [mock_thread(thread_id, preview)],
                                "nextCursor": null,
                                "backwardsCursor": null,
                            }),
                            "thread/loaded/list" => serde_json::json!({
                                "data": [thread_id],
                                "nextCursor": null,
                            }),
                            "thread/read" => {
                                let requested_thread_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("threadId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("thread/read should include threadId");
                                assert_eq!(requested_thread_id, thread_id);
                                serde_json::json!({
                                    "thread": mock_thread(thread_id, preview),
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": preview,
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess"
                                    },
                                    "reasoningEffort": null,
                                })
                            }
                            _ => panic!("unexpected request method: {}", request.method),
                        };

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_disconnect_after_thread_start_server(
        thread_id: &'static str,
        preview: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    let message = read_websocket_message(&mut websocket).await;
                    let JSONRPCMessage::Request(request) = message else {
                        panic!("expected request");
                    };
                    assert_eq!(request.method, "thread/start");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({
                                "thread": mock_thread(thread_id, preview),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_disconnect_after_server_request_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    let message = read_websocket_message(&mut websocket).await;
                    let JSONRPCMessage::Request(request) = message else {
                        panic!("expected request");
                    };
                    assert_eq!(request.method, "thread/start");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({
                                "thread": mock_thread("thread-worker-a", "/tmp/worker-a"),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/worker-a",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("pending-user-input".to_string()),
                            method: "item/tool/requestUserInput".to_string(),
                            params: Some(
                                serde_json::to_value(
                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                        thread_id: "thread-worker-a".to_string(),
                                        turn_id: "turn-worker-a".to_string(),
                                        item_id: "tool-call-worker-a".to_string(),
                                        questions: vec![ToolRequestUserInputQuestion {
                                            id: "mode".to_string(),
                                            header: "Mode".to_string(),
                                            question: "Pick execution mode".to_string(),
                                            is_other: false,
                                            is_secret: false,
                                            options: Some(vec![]),
                                        }],
                                    },
                                )
                                .expect("server request params should serialize"),
                            ),
                            trace: None,
                        }),
                    )
                    .await;

                    sleep(Duration::from_millis(50)).await;
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_session_mutation_server(
        worker_label: &'static str,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        let received_methods = Arc::new(Mutex::new(Vec::new()));
        let received_methods_for_task = Arc::clone(&received_methods);
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let received_methods = Arc::clone(&received_methods_for_task);
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        received_methods.lock().await.push(request.method.clone());

                        let result = match request.method.as_str() {
                            "externalAgentConfig/import" => serde_json::json!({}),
                            "config/value/write" => serde_json::json!({
                                "status": "ok",
                                "version": worker_label,
                                "filePath": "/tmp/shared/config.toml",
                                "overriddenMetadata": null,
                            }),
                            "config/batchWrite" => serde_json::json!({
                                "status": "ok",
                                "version": worker_label,
                                "filePath": "/tmp/shared/config.toml",
                                "overriddenMetadata": null,
                            }),
                            "memory/reset" | "account/logout" => serde_json::json!({}),
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        (format!("ws://{addr}"), received_methods)
    }

    async fn start_mock_remote_multi_connection_discovery_server(
        worker_label: &'static str,
        shared_cwd: &'static str,
        unique_cwd: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "externalAgentConfig/detect" => serde_json::json!({
                                "items": [
                                    {
                                        "itemType": "PLUGINS",
                                        "description": "shared config",
                                        "cwd": shared_cwd,
                                        "details": null,
                                    },
                                    {
                                        "itemType": "PLUGINS",
                                        "description": format!("{worker_label} repo config"),
                                        "cwd": unique_cwd,
                                        "details": null,
                                    },
                                ],
                            }),
                            "skills/list" => serde_json::json!({
                                "data": [{
                                    "cwd": shared_cwd,
                                    "skills": [
                                        {
                                            "name": "shared-skill",
                                            "description": "Shared skill",
                                            "path": format!("{shared_cwd}/shared"),
                                            "scope": "repo",
                                            "enabled": true,
                                        },
                                        {
                                            "name": format!("{worker_label}-skill"),
                                            "description": format!("Skill from {worker_label}"),
                                            "path": format!("{unique_cwd}/skill"),
                                            "scope": "repo",
                                            "enabled": true,
                                        },
                                    ],
                                    "errors": [
                                        {
                                            "path": format!("{shared_cwd}/SKILL.md"),
                                            "message": "shared warning",
                                        },
                                        {
                                            "path": format!("{unique_cwd}/SKILL.md"),
                                            "message": format!("{worker_label} warning"),
                                        },
                                    ],
                                }],
                            }),
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_bootstrap_server(
        requires_openai_auth: bool,
        rate_limits: Option<Vec<(&'static str, &'static str, i32)>>,
        models: Vec<(&'static str, &'static str, bool)>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let models = models.clone();
                let rate_limits = rate_limits.clone();
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "account/read" => serde_json::json!({
                                "account": null,
                                "requiresOpenaiAuth": requires_openai_auth,
                            }),
                            "account/rateLimits/read" => {
                                let snapshots = rate_limits
                                    .as_ref()
                                    .expect("rate limits should be configured for bootstrap test");
                                let primary = snapshots
                                    .first()
                                    .expect("rate limits should include a primary snapshot");
                                let rate_limits_by_limit_id = snapshots
                                    .iter()
                                    .map(|(limit_id, limit_name, used_percent)| {
                                        (
                                            (*limit_id).to_string(),
                                            serde_json::json!({
                                                "limitId": limit_id,
                                                "limitName": limit_name,
                                                "primary": {
                                                    "usedPercent": used_percent,
                                                    "windowMinutes": 300,
                                                    "resetsAt": 1_700_000_000,
                                                },
                                                "secondary": null,
                                                "credits": null,
                                                "planType": null,
                                                "rateLimitReachedType": null,
                                            }),
                                        )
                                    })
                                    .collect::<serde_json::Map<String, serde_json::Value>>();
                                serde_json::json!({
                                    "rateLimits": {
                                        "limitId": primary.0,
                                        "limitName": primary.1,
                                        "primary": {
                                            "usedPercent": primary.2,
                                            "windowMinutes": 300,
                                            "resetsAt": 1_700_000_000,
                                        },
                                        "secondary": null,
                                        "credits": null,
                                        "planType": null,
                                        "rateLimitReachedType": null,
                                    },
                                    "rateLimitsByLimitId": rate_limits_by_limit_id,
                                })
                            }
                            "model/list" => serde_json::json!({
                                "data": models.iter().map(|(id, display_name, is_default)| serde_json::json!({
                                    "id": id,
                                    "model": id,
                                    "upgrade": null,
                                    "upgradeInfo": null,
                                    "availabilityNux": null,
                                    "displayName": display_name,
                                    "description": format!("{display_name} description"),
                                    "hidden": false,
                                    "supportedReasoningEfforts": [{
                                        "reasoningEffort": "medium",
                                        "description": "Balanced"
                                    }],
                                    "defaultReasoningEffort": "medium",
                                    "inputModalities": ["text"],
                                    "supportsPersonality": false,
                                    "additionalSpeedTiers": [],
                                    "isDefault": is_default,
                                })).collect::<Vec<_>>(),
                                "nextCursor": null,
                            }),
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_apps_list_server(
        apps: Vec<(&'static str, &'static str)>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let apps = apps.clone();
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "app/list" => serde_json::json!({
                                "data": apps.iter().map(|(id, name)| serde_json::json!({
                                    "id": id,
                                    "name": name,
                                    "description": format!("{name} description"),
                                    "logoUrl": null,
                                    "logoUrlDark": null,
                                    "distributionChannel": null,
                                    "branding": null,
                                    "appMetadata": null,
                                    "labels": null,
                                    "installUrl": null,
                                    "isAccessible": false,
                                    "isEnabled": true,
                                    "pluginDisplayNames": [],
                                })).collect::<Vec<_>>(),
                                "nextCursor": null,
                            }),
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_mcp_server_status_list_server(
        names: Vec<&'static str>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let names = names.clone();
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "mcpServerStatus/list" => serde_json::json!({
                                "data": names.iter().map(|name| serde_json::json!({
                                    "name": name,
                                    "tools": {},
                                    "resources": [],
                                    "resourceTemplates": [],
                                    "authStatus": "bearerToken",
                                })).collect::<Vec<_>>(),
                                "nextCursor": null,
                            }),
                            method => panic!("unexpected request method: {method}"),
                        };
                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_skills_changed_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Notification(
                            codex_app_server_protocol::JSONRPCNotification {
                                method: "skills/changed".to_string(),
                                params: Some(serde_json::json!({})),
                            },
                        ),
                    )
                    .await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        match request.method.as_str() {
                            "skills/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "cwd": "/tmp/shared-repo",
                                                "skills": [],
                                                "errors": [],
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "skills/changed".to_string(),
                                            params: Some(serde_json::json!({})),
                                        },
                                    ),
                                )
                                .await;
                            }
                            method => panic!("unexpected request method: {method}"),
                        }
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_state_notification_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;
                    for notification in [
                        serde_json::json!({
                            "method": "account/updated",
                            "params": {
                                "authMode": null,
                                "planType": null,
                            },
                        }),
                        serde_json::json!({
                            "method": "account/rateLimits/updated",
                            "params": {
                                "rateLimits": {
                                    "limitId": null,
                                    "limitName": null,
                                    "primary": null,
                                    "secondary": null,
                                    "credits": null,
                                    "planType": null,
                                    "rateLimitReachedType": null,
                                },
                            },
                        }),
                        serde_json::json!({
                            "method": "app/list/updated",
                            "params": {
                                "data": [{
                                    "id": "calendar",
                                    "name": "calendar",
                                    "description": null,
                                    "logoUrl": null,
                                    "logoUrlDark": null,
                                    "distributionChannel": null,
                                    "branding": null,
                                    "appMetadata": null,
                                    "labels": null,
                                    "installUrl": null,
                                }],
                            },
                        }),
                    ] {
                        write_websocket_message(
                            &mut websocket,
                            serde_json::from_value::<JSONRPCMessage>(notification)
                                .expect("notification should decode"),
                        )
                        .await;
                    }

                    let message = read_websocket_message(&mut websocket).await;
                    panic!("unexpected message: {message:?}");
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_workflow_server(
        thread_id: &'static str,
        preview: &'static str,
        turn_id: &'static str,
        delta: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        match request.method.as_str() {
                            "thread/start" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": mock_thread(thread_id, preview),
                                            "model": "gpt-5",
                                            "modelProvider": "openai",
                                            "serviceTier": null,
                                            "cwd": preview,
                                            "instructionSources": [],
                                            "approvalPolicy": "never",
                                            "approvalsReviewer": "user",
                                            "sandbox": {
                                                "type": "dangerFullAccess"
                                            },
                                            "reasoningEffort": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "turn/start" => {
                                let requested_thread_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("threadId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("turn/start should include threadId");
                                assert_eq!(requested_thread_id, thread_id);

                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "turn": mock_turn(turn_id, "inProgress"),
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "thread/status/changed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "status": {
                                                    "type": "active",
                                                    "activeFlags": [],
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/started".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(turn_id, "inProgress"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/agentMessage/delta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("msg-{thread_id}"),
                                                "delta": delta,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/summaryTextDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-summary-{thread_id}"),
                                                "delta": format!("summary {thread_id}"),
                                                "summaryIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/reasoning/textDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("reasoning-{thread_id}"),
                                                "delta": format!("reasoning {thread_id}"),
                                                "contentIndex": 0,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/commandExecution/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("exec-{thread_id}"),
                                                "delta": format!("stdout {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/fileChange/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "itemId": format!("patch-{thread_id}"),
                                                "delta": format!("patch {thread_id}"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "turn/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turn": mock_turn(turn_id, "completed"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            _ => panic!("unexpected request method: {}", request.method),
                        }
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_plugin_server(
        marketplace_name: &'static str,
        preview: &'static str,
        plugin_name: &'static str,
        description: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        let plugin_id = format!("{plugin_name}@{marketplace_name}");
        let marketplace_path = format!("{preview}/marketplace.json");
        let plugin_path = format!("{preview}/plugins/{plugin_name}");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                let plugin_id = plugin_id.clone();
                let marketplace_path = marketplace_path.clone();
                let plugin_path = plugin_path.clone();
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    let mut plugin_installed = false;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        match request.method.as_str() {
                            "plugin/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "marketplaces": [{
                                                "name": marketplace_name,
                                                "path": marketplace_path.clone(),
                                                "interface": {
                                                    "displayName": marketplace_name,
                                                },
                                                "plugins": [{
                                                    "id": plugin_id.clone(),
                                                    "name": plugin_name,
                                                    "source": {
                                                        "type": "local",
                                                        "path": plugin_path.clone(),
                                                    },
                                                    "installed": plugin_installed,
                                                    "enabled": plugin_installed,
                                                    "installPolicy": "AVAILABLE",
                                                    "authPolicy": "ON_INSTALL",
                                                    "interface": {
                                                        "displayName": plugin_name,
                                                        "shortDescription": format!("{plugin_name} short description"),
                                                        "longDescription": null,
                                                        "developerName": null,
                                                        "category": "tools",
                                                        "capabilities": ["commands"],
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
                                            "featuredPluginIds": [plugin_id.clone()],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/read" => {
                                let plugin_read_params: PluginReadParams = serde_json::from_value(
                                    request
                                        .params
                                        .clone()
                                        .expect("plugin/read params should exist"),
                                )
                                .expect("plugin/read params should decode");
                                if plugin_read_params.plugin_name != plugin_name
                                    || plugin_read_params.marketplace_path.as_ref().is_none_or(
                                        |path| {
                                            path.as_path()
                                                != std::path::Path::new(&marketplace_path)
                                        },
                                    )
                                {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "plugin": {
                                                "marketplaceName": marketplace_name,
                                                "marketplacePath": marketplace_path.clone(),
                                                "summary": {
                                                    "id": plugin_id.clone(),
                                                    "name": plugin_name,
                                                    "source": {
                                                        "type": "local",
                                                        "path": plugin_path.clone(),
                                                    },
                                                    "installed": plugin_installed,
                                                    "enabled": plugin_installed,
                                                    "installPolicy": "AVAILABLE",
                                                    "authPolicy": "ON_INSTALL",
                                                    "interface": {
                                                        "displayName": plugin_name,
                                                        "shortDescription": format!("{plugin_name} short description"),
                                                        "longDescription": null,
                                                        "developerName": null,
                                                        "category": "tools",
                                                        "capabilities": ["commands"],
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
                                                },
                                                "description": description,
                                                "skills": [],
                                                "apps": [],
                                                "mcpServers": [],
                                            },
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/install" => {
                                let plugin_install_params: PluginInstallParams =
                                    serde_json::from_value(
                                        request
                                            .params
                                            .clone()
                                            .expect("plugin/install params should exist"),
                                    )
                                    .expect("plugin/install params should decode");
                                if plugin_install_params.plugin_name != plugin_name
                                    || plugin_install_params.marketplace_path.as_ref().is_none_or(
                                        |path| {
                                            path.as_path()
                                                != std::path::Path::new(&marketplace_path)
                                        },
                                    )
                                {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                plugin_installed = true;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "authPolicy": "ON_INSTALL",
                                            "appsNeedingAuth": [],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/uninstall" => {
                                let plugin_uninstall_params: PluginUninstallParams =
                                    serde_json::from_value(
                                        request
                                            .params
                                            .clone()
                                            .expect("plugin/uninstall params should exist"),
                                    )
                                    .expect("plugin/uninstall params should decode");
                                if plugin_uninstall_params.plugin_id != plugin_id {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Error(JSONRPCError {
                                            id: request.id,
                                            error: JSONRPCErrorError {
                                                code: -32602,
                                                message: "plugin not found on worker".to_string(),
                                                data: None,
                                            },
                                        }),
                                    )
                                    .await;
                                    continue;
                                }
                                plugin_installed = false;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                            }
                            _ => panic!("unexpected request method: {}", request.method),
                        }
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_server_request_server(
        thread_id: &'static str,
        preview: &'static str,
        expected_answer: &'static str,
        previous_account_id: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;

                    let JSONRPCMessage::Request(thread_start_request) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected thread/start request");
                    };
                    assert_eq!(thread_start_request.method, "thread/start");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: thread_start_request.id,
                            result: serde_json::json!({
                                "thread": mock_thread(thread_id, preview),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-server-request".to_string()),
                            method: "item/tool/requestUserInput".to_string(),
                            params: Some(
                                serde_json::to_value(
                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                        thread_id: thread_id.to_string(),
                                        turn_id: "turn-remote-workflow".to_string(),
                                        item_id: "tool-call-remote-workflow".to_string(),
                                        questions: vec![ToolRequestUserInputQuestion {
                                            id: "mode".to_string(),
                                            header: "Mode".to_string(),
                                            question: "Pick execution mode".to_string(),
                                            is_other: false,
                                            is_secret: false,
                                            options: Some(vec![]),
                                        }],
                                    },
                                )
                                .expect("server request params should serialize"),
                            ),
                            trace: None,
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Response(response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected server request response");
                    };
                    assert_eq!(
                        response.id,
                        RequestId::String("shared-server-request".to_string())
                    );
                    assert_eq!(
                        response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": [expected_answer],
                                },
                            },
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-command-request".to_string()),
                            method: "item/commandExecution/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": thread_id,
                                "turnId": "turn-remote-workflow",
                                "itemId": "cmd-remote-workflow",
                                "command": "pwd",
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(command_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected command approval response");
                    };
                    assert_eq!(
                        command_response.id,
                        RequestId::String("shared-command-request".to_string())
                    );
                    assert_eq!(
                        command_response.result,
                        serde_json::json!({
                            "decision": "accept",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-file-request".to_string()),
                            method: "item/fileChange/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": thread_id,
                                "turnId": "turn-remote-workflow",
                                "itemId": "file-remote-workflow",
                                "reason": "Need to write changes",
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(file_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected file approval response");
                    };
                    assert_eq!(
                        file_response.id,
                        RequestId::String("shared-file-request".to_string())
                    );
                    assert_eq!(
                        file_response.result,
                        serde_json::json!({
                            "decision": "accept",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-mcp-request".to_string()),
                            method: "mcpServer/elicitation/request".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": thread_id,
                                "turnId": "turn-remote-workflow",
                                "serverName": "mock-mcp",
                                "mode": "form",
                                "_meta": null,
                                "message": "Allow mock action?",
                                "requestedSchema": {
                                    "type": "object",
                                    "properties": {
                                        "confirmed": {
                                            "type": "boolean",
                                        },
                                    },
                                    "required": ["confirmed"],
                                },
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(mcp_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected mcp elicitation response");
                    };
                    assert_eq!(
                        mcp_response.id,
                        RequestId::String("shared-mcp-request".to_string())
                    );
                    assert_eq!(
                        mcp_response.result,
                        serde_json::json!({
                            "action": "accept",
                            "content": {
                                "confirmed": true,
                            },
                            "_meta": null,
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-permissions-request".to_string()),
                            method: "item/permissions/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": thread_id,
                                "turnId": "turn-remote-workflow",
                                "itemId": "perm-remote-workflow",
                                "reason": "Need wider permissions",
                                "permissions": {
                                    "fileSystem": null,
                                    "network": {
                                        "enabled": true,
                                    },
                                },
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(permissions_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected permissions approval response");
                    };
                    assert_eq!(
                        permissions_response.id,
                        RequestId::String("shared-permissions-request".to_string())
                    );
                    assert_eq!(
                        permissions_response.result,
                        serde_json::json!({
                            "permissions": {
                                "network": {
                                    "enabled": true,
                                },
                            },
                            "scope": "turn",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("shared-chatgpt-refresh-request".to_string()),
                            method: "account/chatgptAuthTokens/refresh".to_string(),
                            params: Some(serde_json::json!({
                                "reason": "unauthorized",
                                "previousAccountId": previous_account_id,
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(refresh_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected chatgpt refresh response");
                    };
                    assert_eq!(
                        refresh_response.id,
                        RequestId::String("shared-chatgpt-refresh-request".to_string())
                    );
                    assert_eq!(
                        refresh_response.result,
                        serde_json::json!({
                            "accessToken": format!("access-token-{previous_account_id}"),
                            "chatgptAccountId": previous_account_id,
                            "chatgptPlanType": "pro",
                        })
                    );

                    while let Some(frame) = websocket.next().await {
                        match frame.expect("follow-up frame should decode") {
                            Message::Close(_) => break,
                            Message::Ping(payload) => {
                                websocket
                                    .send(Message::Pong(payload))
                                    .await
                                    .expect("pong should send");
                            }
                            Message::Text(_)
                            | Message::Binary(_)
                            | Message::Pong(_)
                            | Message::Frame(_) => {}
                        }
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_multi_connection_mutation_server(
        thread_id: &'static str,
        preview: &'static str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");
                    expect_remote_initialize(&mut websocket).await;
                    let mut thread_name: Option<String> = None;

                    loop {
                        let message = read_websocket_message(&mut websocket).await;
                        let JSONRPCMessage::Request(request) = message else {
                            panic!("expected request");
                        };
                        let result = match request.method.as_str() {
                            "thread/start" => serde_json::json!({
                                "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                            "thread/name/set" => {
                                let requested_thread_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("threadId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("thread/name/set should include threadId");
                                assert_eq!(requested_thread_id, thread_id);
                                let name = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("name"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("thread/name/set should include name");
                                thread_name = Some(name.to_string());
                                serde_json::json!({})
                            }
                            "thread/read" => {
                                let requested_thread_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("threadId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("thread/read should include threadId");
                                assert_eq!(requested_thread_id, thread_id);
                                serde_json::json!({
                                    "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": preview,
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess"
                                    },
                                    "reasoningEffort": null,
                                })
                            }
                            "thread/list" => serde_json::json!({
                                "data": [mock_thread_with_name(thread_id, preview, thread_name.as_deref())],
                                "nextCursor": null,
                                "backwardsCursor": null,
                            }),
                            _ => panic!("unexpected request method: {}", request.method),
                        };

                        write_websocket_message(
                            &mut websocket,
                            JSONRPCMessage::Response(JSONRPCResponse {
                                id: request.id,
                                result,
                            }),
                        )
                        .await;
                    }
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_server_request_roundtrip() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");

                    let JSONRPCMessage::Request(request) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialize request");
                    };
                    assert_eq!(request.method, "initialize");
                    let initialize_params: codex_app_server_protocol::InitializeParams =
                        serde_json::from_value(
                            request
                                .params
                                .clone()
                                .expect("initialize should include params"),
                        )
                        .expect("initialize params should decode");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({}),
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Notification(notification) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");

                    if initialize_params.client_info.name != "codex-gateway-test" {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        return;
                    }

                    let JSONRPCMessage::Request(thread_start_request) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected thread/start request");
                    };
                    assert_eq!(thread_start_request.method, "thread/start");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: thread_start_request.id.clone(),
                            result: serde_json::json!({
                                "thread": mock_thread("thread-remote-workflow", "/tmp/remote-project"),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/remote-project",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                            method: "thread/started".to_string(),
                            params: Some(serde_json::json!({
                                "thread": mock_thread("thread-remote-workflow", "/tmp/remote-project"),
                            })),
                        }),
                    )
                    .await;

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-user-input".to_string()),
                            method: "item/tool/requestUserInput".to_string(),
                            params: Some(
                                serde_json::to_value(
                                    codex_app_server_protocol::ToolRequestUserInputParams {
                                        thread_id: "thread-remote-workflow".to_string(),
                                        turn_id: "turn-remote-workflow".to_string(),
                                        item_id: "tool-call-remote-workflow".to_string(),
                                        questions: vec![ToolRequestUserInputQuestion {
                                            id: "mode".to_string(),
                                            header: "Mode".to_string(),
                                            question: "Pick execution mode".to_string(),
                                            is_other: false,
                                            is_secret: false,
                                            options: Some(vec![]),
                                        }],
                                    },
                                )
                                .expect("server request params should serialize"),
                            ),
                            trace: None,
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Response(response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected server request response");
                    };
                    assert_eq!(response.id, RequestId::String("srv-user-input".to_string()));
                    assert_eq!(
                        response.result,
                        serde_json::json!({
                            "answers": {
                                "mode": {
                                    "answers": ["safe"],
                                },
                            },
                        })
                    );
                });
            }
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_multiple_server_request_roundtrips() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept should succeed");
                tokio::spawn(async move {
                    let mut websocket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("websocket upgrade should succeed");

                    let JSONRPCMessage::Request(request) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialize request");
                    };
                    assert_eq!(request.method, "initialize");
                    let initialize_params: codex_app_server_protocol::InitializeParams =
                        serde_json::from_value(
                            request
                                .params
                                .clone()
                                .expect("initialize should include params"),
                        )
                        .expect("initialize params should decode");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result: serde_json::json!({}),
                        }),
                    )
                    .await;

                    let JSONRPCMessage::Notification(notification) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected initialized notification");
                    };
                    assert_eq!(notification.method, "initialized");

                    if initialize_params.client_info.name != "codex-gateway-test" {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        return;
                    }

                    let JSONRPCMessage::Request(thread_start_request) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected thread/start request");
                    };
                    assert_eq!(thread_start_request.method, "thread/start");
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: thread_start_request.id,
                            result: serde_json::json!({
                                "thread": mock_thread("thread-remote-workflow", "/tmp/remote-project"),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/remote-project",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            }),
                        }),
                    )
                    .await;
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                            method: "thread/started".to_string(),
                            params: Some(serde_json::json!({
                                "thread": mock_thread("thread-remote-workflow", "/tmp/remote-project"),
                            })),
                        }),
                    )
                    .await;

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-command".to_string()),
                            method: "item/commandExecution/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": "thread-remote-workflow",
                                "turnId": "turn-remote-workflow",
                                "itemId": "cmd-remote-workflow",
                                "command": "pwd",
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(command_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected command approval response");
                    };
                    assert_eq!(
                        command_response.id,
                        RequestId::String("srv-command".to_string())
                    );
                    assert_eq!(
                        command_response.result,
                        serde_json::json!({
                            "decision": "accept",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-file".to_string()),
                            method: "item/fileChange/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": "thread-remote-workflow",
                                "turnId": "turn-remote-workflow",
                                "itemId": "file-remote-workflow",
                                "reason": "Need to write changes",
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(file_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected file approval response");
                    };
                    assert_eq!(file_response.id, RequestId::String("srv-file".to_string()));
                    assert_eq!(
                        file_response.result,
                        serde_json::json!({
                            "decision": "accept",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-mcp-elicitation".to_string()),
                            method: "mcpServer/elicitation/request".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": "thread-remote-workflow",
                                "turnId": "turn-remote-workflow",
                                "serverName": "mock-mcp",
                                "mode": "form",
                                "_meta": null,
                                "message": "Allow mock action?",
                                "requestedSchema": {
                                    "type": "object",
                                    "properties": {
                                        "confirmed": {
                                            "type": "boolean",
                                        },
                                    },
                                    "required": ["confirmed"],
                                },
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(mcp_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected mcp elicitation response");
                    };
                    assert_eq!(
                        mcp_response.id,
                        RequestId::String("srv-mcp-elicitation".to_string())
                    );
                    assert_eq!(
                        mcp_response.result,
                        serde_json::json!({
                            "action": "accept",
                            "content": {
                                "confirmed": true,
                            },
                            "_meta": null,
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-permissions".to_string()),
                            method: "item/permissions/requestApproval".to_string(),
                            params: Some(serde_json::json!({
                                "threadId": "thread-remote-workflow",
                                "turnId": "turn-remote-workflow",
                                "itemId": "perm-remote-workflow",
                                "reason": "Need wider permissions",
                                "permissions": {
                                    "network": {
                                        "enabled": true,
                                    },
                                },
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(permissions_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected permissions approval response");
                    };
                    assert_eq!(
                        permissions_response.id,
                        RequestId::String("srv-permissions".to_string())
                    );
                    assert_eq!(
                        permissions_response.result,
                        serde_json::json!({
                            "permissions": {
                                "fileSystem": null,
                                "network": {
                                    "enabled": true,
                                },
                            },
                            "scope": "turn",
                        })
                    );

                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Request(codex_app_server_protocol::JSONRPCRequest {
                            id: RequestId::String("srv-chatgpt-refresh".to_string()),
                            method: "account/chatgptAuthTokens/refresh".to_string(),
                            params: Some(serde_json::json!({
                                "reason": "unauthorized",
                                "previousAccountId": "acct-123",
                            })),
                            trace: None,
                        }),
                    )
                    .await;
                    let JSONRPCMessage::Response(refresh_response) =
                        read_websocket_message(&mut websocket).await
                    else {
                        panic!("expected chatgpt refresh response");
                    };
                    assert_eq!(
                        refresh_response.id,
                        RequestId::String("srv-chatgpt-refresh".to_string())
                    );
                    assert_eq!(
                        refresh_response.result,
                        serde_json::json!({
                            "accessToken": "access-token-1",
                            "chatgptAccountId": "acct-123",
                            "chatgptPlanType": "pro",
                        })
                    );
                });
            }
        });
        format!("ws://{addr}")
    }

    struct MockRemoteServerOptions {
        expected_auth_token: Option<String>,
        thread_id: &'static str,
        preview: &'static str,
        close_after_first_request: bool,
    }

    async fn start_mock_remote_server_with_options(options: MockRemoteServerOptions) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let expected_auth_token = options.expected_auth_token.clone();
            let thread_id = options.thread_id;
            let preview = options.preview;
            let close_after_first_request = options.close_after_first_request;
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = accept_hdr_async(
                stream,
                move |request: &WebSocketRequest, response: WebSocketResponse| {
                    let provided_auth_token = request
                        .headers()
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned);
                    let expected_header = expected_auth_token
                        .as_ref()
                        .map(|token| format!("Bearer {token}"));
                    assert_eq!(provided_auth_token, expected_header);
                    Ok(response)
                },
            )
            .await
            .expect("websocket upgrade should succeed");

            expect_remote_initialize(&mut websocket).await;
            let mut handled_requests = 0usize;
            loop {
                let message = read_websocket_message(&mut websocket).await;
                let JSONRPCMessage::Request(request) = message else {
                    panic!("expected request");
                };
                let result = match request.method.as_str() {
                    "thread/start" => serde_json::json!({
                        "thread": mock_thread(thread_id, preview),
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": preview,
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess"
                        },
                        "reasoningEffort": null,
                    }),
                    "thread/read" => serde_json::json!({
                        "thread": mock_thread(thread_id, preview),
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": preview,
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess"
                        },
                        "reasoningEffort": null,
                    }),
                    "thread/list" => serde_json::json!({
                        "data": [mock_thread(thread_id, preview)],
                        "nextCursor": null,
                        "backwardsCursor": null,
                    }),
                    method => panic!("unexpected request method: {method}"),
                };
                write_websocket_message(
                    &mut websocket,
                    JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result,
                    }),
                )
                .await;
                handled_requests += 1;
                if close_after_first_request && handled_requests == 1 {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                    break;
                }
            }
        });
        format!("ws://{addr}")
    }

    fn mock_thread(thread_id: &str, preview: &str) -> serde_json::Value {
        mock_thread_with_name(thread_id, preview, None)
    }

    fn mock_thread_with_name(
        thread_id: &str,
        preview: &str,
        name: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": thread_id,
            "forkedFromId": null,
            "preview": preview,
            "ephemeral": true,
            "modelProvider": "openai",
            "createdAt": 1,
            "updatedAt": if thread_id.ends_with("-b") { 2 } else { 1 },
            "status": {
                "type": "idle"
            },
            "path": null,
            "cwd": preview,
            "cliVersion": "0.0.0-test",
            "source": "vscode",
            "agentNickname": null,
            "agentRole": null,
            "gitInfo": null,
            "name": name,
            "turns": [],
        })
    }

    fn mock_turn(turn_id: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "id": turn_id,
            "items": [],
            "status": status,
            "error": null,
            "startedAt": 1,
            "completedAt": if status == "completed" { Some(2) } else { None::<i64> },
            "durationMs": if status == "completed" { Some(1000) } else { None::<i64> },
        })
    }

    async fn expect_remote_initialize<S>(websocket: &mut tokio_tungstenite::WebSocketStream<S>)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let JSONRPCMessage::Request(request) = read_websocket_message(websocket).await else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        write_websocket_message(
            websocket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({}),
            }),
        )
        .await;

        let JSONRPCMessage::Notification(notification) = read_websocket_message(websocket).await
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    }

    async fn start_mock_exec_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener addr");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection should accept");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialize request");
            };
            assert_eq!(request.method, "initialize");
            write_websocket_message(
                &mut websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({
                        "sessionId": "exec-session-1"
                    }),
                }),
            )
            .await;

            let JSONRPCMessage::Notification(notification) =
                read_websocket_message(&mut websocket).await
            else {
                panic!("expected initialized notification");
            };
            assert_eq!(notification.method, "initialized");
        });
        format!("ws://{addr}")
    }

    async fn read_websocket_message<S>(
        websocket: &mut tokio_tungstenite::WebSocketStream<S>,
    ) -> JSONRPCMessage
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            let frame = websocket
                .next()
                .await
                .expect("frame should be available")
                .expect("frame should decode");
            match frame {
                Message::Text(text) => {
                    return serde_json::from_str::<JSONRPCMessage>(&text)
                        .expect("text frame should be valid JSON-RPC");
                }
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue;
                }
                Message::Close(_) => panic!("unexpected close frame"),
            }
        }
    }

    async fn write_websocket_message<S>(
        websocket: &mut tokio_tungstenite::WebSocketStream<S>,
        message: JSONRPCMessage,
    ) where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        websocket
            .send(Message::Text(
                serde_json::to_string(&message)
                    .expect("message should serialize")
                    .into(),
            ))
            .await
            .expect("message should send");
    }

    struct MockResponsesServer {
        uri: String,
        shutdown: Option<oneshot::Sender<()>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl MockResponsesServer {
        async fn shutdown(mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            let _ = self.task.await;
        }
    }

    async fn start_mock_responses_server_repeating_assistant(message: &str) -> MockResponsesServer {
        let body = mock_responses_sse_body(message);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let app = axum::Router::new().route(
                "/v1/responses",
                axum::routing::post(move || {
                    let body = body.clone();
                    async move {
                        (
                            [
                                (axum::http::header::CONTENT_TYPE, "text/event-stream"),
                                (axum::http::header::CACHE_CONTROL, "no-cache"),
                            ],
                            body,
                        )
                    }
                }),
            );
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        MockResponsesServer {
            uri: format!("http://{addr}"),
            shutdown: Some(shutdown_tx),
            task,
        }
    }

    async fn start_mock_responses_server_sequence(bodies: Vec<String>) -> MockResponsesServer {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener addr");
        let responses = Arc::new(Mutex::new(VecDeque::from(bodies)));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let app = axum::Router::new().route(
                "/v1/responses",
                axum::routing::post(move || {
                    let responses = responses.clone();
                    async move {
                        let body = responses
                            .lock()
                            .await
                            .pop_front()
                            .expect("mock responses sequence should have another body");
                        (
                            [
                                (axum::http::header::CONTENT_TYPE, "text/event-stream"),
                                (axum::http::header::CACHE_CONTROL, "no-cache"),
                            ],
                            body,
                        )
                    }
                }),
            );
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        MockResponsesServer {
            uri: format!("http://{addr}"),
            shutdown: Some(shutdown_tx),
            task,
        }
    }

    fn mock_responses_sse_body(message: &str) -> String {
        format!(
            "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"id\":\"msg-1\",\"content\":[{{\"type\":\"output_text\",\"text\":\"\"}}]}}}}\n\n\
event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{message:?}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"message\",\"role\":\"assistant\",\"id\":\"msg-1\",\"content\":[{{\"type\":\"output_text\",\"text\":{message:?}}}]}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
        )
    }

    fn mock_responses_request_user_input_sse_body(call_id: &str) -> String {
        format!(
            "event: response.created\n\
data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"resp-1\"}}}}\n\n\
event: response.output_item.added\n\
data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"function_call\",\"id\":{call_id:?},\"call_id\":{call_id:?},\"name\":\"request_user_input\",\"arguments\":\"{{\\\"questions\\\":[{{\\\"id\\\":\\\"confirm_path\\\",\\\"header\\\":\\\"Confirm\\\",\\\"question\\\":\\\"Proceed with the plan?\\\",\\\"options\\\":[{{\\\"label\\\":\\\"Yes (Recommended)\\\",\\\"description\\\":\\\"Continue the current plan.\\\"}},{{\\\"label\\\":\\\"No\\\",\\\"description\\\":\\\"Stop and revisit the approach.\\\"}}]}}]}}\"}}}}\n\n\
event: response.output_item.done\n\
data: {{\"type\":\"response.output_item.done\",\"item\":{{\"type\":\"function_call\",\"id\":{call_id:?},\"call_id\":{call_id:?},\"name\":\"request_user_input\",\"arguments\":\"{{\\\"questions\\\":[{{\\\"id\\\":\\\"confirm_path\\\",\\\"header\\\":\\\"Confirm\\\",\\\"question\\\":\\\"Proceed with the plan?\\\",\\\"options\\\":[{{\\\"label\\\":\\\"Yes (Recommended)\\\",\\\"description\\\":\\\"Continue the current plan.\\\"}},{{\\\"label\\\":\\\"No\\\",\\\"description\\\":\\\"Stop and revisit the approach.\\\"}}]}}]}}\"}}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp-1\",\"usage\":{{\"input_tokens\":0,\"input_tokens_details\":null,\"output_tokens\":0,\"output_tokens_details\":null,\"total_tokens\":0}}}}}}\n\n"
        )
    }

    fn mock_responses_namespaced_function_call_sse_body(
        call_id: &str,
        namespace: &str,
        name: &str,
        arguments: &str,
    ) -> String {
        let created = serde_json::json!({
            "type": "response.created",
            "response": {
                "id": "resp-1",
            },
        });
        let function_call = serde_json::json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "call_id": call_id,
                "namespace": namespace,
                "name": name,
                "arguments": arguments,
            },
        });
        let completed = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp-1",
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": null,
                    "output_tokens": 0,
                    "output_tokens_details": null,
                    "total_tokens": 0,
                },
            },
        });

        format!(
            "event: response.created\n\
data: {}\n\n\
event: response.output_item.done\n\
data: {}\n\n\
event: response.completed\n\
data: {}\n\n",
            serde_json::to_string(&created).expect("created event should serialize"),
            serde_json::to_string(&function_call).expect("function call should serialize"),
            serde_json::to_string(&completed).expect("completed event should serialize"),
        )
    }

    #[derive(Clone)]
    struct EmbeddedAppsServerState {
        expected_bearer: String,
        expected_account_id: String,
    }

    #[derive(Clone, Default)]
    struct EmbeddedElicitationAppsMcpServer;

    impl ServerHandler for EmbeddedElicitationAppsMcpServer {
        fn get_info(&self) -> ServerInfo {
            ServerInfo {
                protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
                capabilities: ServerCapabilities::builder().enable_tools().build(),
                ..ServerInfo::default()
            }
        }

        async fn list_tools(
            &self,
            _request: Option<rmcp::model::PaginatedRequestParams>,
            _context: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, rmcp::ErrorData> {
            let input_schema: JsonObject = serde_json::from_value(serde_json::json!({
                "type": "object",
                "additionalProperties": false
            }))
            .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

            let mut tool = Tool::new(
                Cow::Borrowed(EMBEDDED_TOOL_NAME),
                Cow::Borrowed("Confirm a calendar action."),
                Arc::new(input_schema),
            );
            tool.annotations = Some(ToolAnnotations::new().read_only(true));

            let mut meta = Meta::new();
            meta.0.insert(
                "connector_id".to_string(),
                serde_json::json!(EMBEDDED_CONNECTOR_ID),
            );
            meta.0.insert(
                "connector_name".to_string(),
                serde_json::json!(EMBEDDED_CONNECTOR_NAME),
            );
            tool.meta = Some(meta);

            Ok(ListToolsResult {
                tools: vec![tool],
                next_cursor: None,
                meta: None,
            })
        }

        async fn call_tool(
            &self,
            _request: CallToolRequestParams,
            context: RequestContext<RoleServer>,
        ) -> Result<CallToolResult, rmcp::ErrorData> {
            let requested_schema = ElicitationSchema::builder()
                .required_property("confirmed", PrimitiveSchema::Boolean(BooleanSchema::new()))
                .build()
                .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

            let result = context
                .peer
                .create_elicitation(CreateElicitationRequestParams::FormElicitationParams {
                    meta: None,
                    message: EMBEDDED_ELICITATION_MESSAGE.to_string(),
                    requested_schema,
                })
                .await
                .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;

            let output = match result.action {
                ElicitationAction::Accept => {
                    assert_eq!(
                        result.content,
                        Some(serde_json::json!({
                            "confirmed": true,
                        }))
                    );
                    "accepted"
                }
                ElicitationAction::Decline => "declined",
                ElicitationAction::Cancel => "cancelled",
            };

            Ok(CallToolResult::success(vec![Content::text(output)]))
        }
    }

    async fn start_embedded_gateway_apps_server() -> anyhow::Result<(String, JoinHandle<()>)> {
        let state = Arc::new(EmbeddedAppsServerState {
            expected_bearer: "Bearer chatgpt-token".to_string(),
            expected_account_id: "account-123".to_string(),
        });

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let mcp_service = StreamableHttpService::new(
            move || Ok(EmbeddedElicitationAppsMcpServer),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

        let router = Router::new()
            .route(
                "/connectors/directory/list",
                get(list_embedded_directory_connectors),
            )
            .route(
                "/connectors/directory/list_workspace",
                get(list_embedded_directory_connectors),
            )
            .with_state(state)
            .nest_service("/api/codex/apps", mcp_service);

        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Ok((format!("http://{addr}"), handle))
    }

    async fn list_embedded_directory_connectors(
        State(state): State<Arc<EmbeddedAppsServerState>>,
        headers: HeaderMap,
        uri: Uri,
    ) -> Result<Json<serde_json::Value>, StatusCode> {
        let bearer_ok = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == state.expected_bearer);
        let account_ok = headers
            .get("chatgpt-account-id")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == state.expected_account_id);
        let external_logos_ok = uri
            .query()
            .is_some_and(|query| query.split('&').any(|pair| pair == "external_logos=true"));

        if !bearer_ok || !account_ok {
            Err(StatusCode::UNAUTHORIZED)
        } else if !external_logos_ok {
            Err(StatusCode::BAD_REQUEST)
        } else {
            Ok(Json(serde_json::json!({
                "apps": [{
                    "id": EMBEDDED_CONNECTOR_ID,
                    "name": EMBEDDED_CONNECTOR_NAME,
                    "description": "Calendar connector",
                    "logo_url": null,
                    "logo_url_dark": null,
                    "distribution_channel": null,
                    "branding": null,
                    "app_metadata": null,
                    "labels": null,
                    "install_url": null,
                    "is_accessible": false,
                    "is_enabled": true
                }],
                "next_token": null
            })))
        }
    }

    fn write_mock_responses_config_toml(
        codex_home: &std::path::Path,
        server_uri: &str,
    ) -> std::io::Result<()> {
        let config_toml = codex_home.join("config.toml");
        std::fs::write(
            config_toml,
            format!(
                r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
            ),
        )
    }

    fn write_embedded_mcp_config_toml(
        codex_home: &std::path::Path,
        responses_server_uri: &str,
        apps_server_url: &str,
    ) -> std::io::Result<()> {
        std::fs::write(
            codex_home.join("config.toml"),
            format!(
                r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"
chatgpt_base_url = "{apps_server_url}"
mcp_oauth_credentials_store = "file"

[features]
apps = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{responses_server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
            ),
        )
    }

    fn assert_remote_client_shutdown(result: std::io::Result<()>) {
        match result {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(err) => panic!("shutdown should complete: {err:?}"),
        }
    }
}
