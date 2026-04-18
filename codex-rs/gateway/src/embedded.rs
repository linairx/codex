use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayServerRequest;
use crate::config::GatewayConfig;
use crate::config::GatewayRemoteWorkerConfig;
use crate::config::GatewayRuntimeMode;
use crate::event::GatewayEvent;
use crate::northbound::http::router_with_observability;
use crate::observability::GatewayObservability;
use crate::observability::init_tracing;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_runtime::RemoteWorkerGatewayRuntime;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::AppServerGatewayRuntime;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayScopeRegistry;
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
                exec_server_url = gateway_config.exec_server_url.as_deref(),
                "starting gateway with embedded app-server runtime"
            );
            let config_warnings = config
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
            let app_server = AppServerClient::InProcess(
                InProcessAppServerClient::start(InProcessClientStartArgs {
                    arg0_paths,
                    config: Arc::new(config),
                    cli_overrides,
                    loader_overrides,
                    cloud_requirements: CloudRequirementsLoader::default(),
                    feedback: CodexFeedback::new(),
                    log_db: None,
                    environment_manager,
                    config_warnings,
                    session_source: gateway_config.session_source.clone(),
                    enable_codex_api_key_env: gateway_config.enable_codex_api_key_env,
                    client_name: gateway_config.client_name.clone(),
                    client_version: gateway_config.client_version.clone(),
                    experimental_api: gateway_config.experimental_api,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: gateway_config.channel_capacity,
                })
                .await?,
            );

            start_single_runtime_gateway_http_server(gateway_config, otel, app_server).await
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
                remote_worker_count = gateway_config
                    .remote_runtime
                    .as_ref()
                    .map_or(0, |remote_runtime| remote_runtime.workers.len()),
                "starting gateway with remote app-server workers"
            );
            start_remote_runtime_gateway_http_server(gateway_config, otel).await
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
) -> io::Result<GatewayServer> {
    let (events_tx, _events_rx) =
        tokio::sync::broadcast::channel(gateway_config.event_buffer_capacity);
    let execution_mode = gateway_execution_mode(&gateway_config);
    let request_handle = app_server.request_handle();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
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
    ));
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let (event_shutdown_tx, event_shutdown_rx) = oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                observability,
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
        AppServerGatewayRuntime::new(request_handle, execution_mode, events_tx, scope_registry),
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
    let runtime: Arc<dyn GatewayRuntime> = Arc::new(
        RemoteWorkerGatewayRuntime::new(
            workers.clone(),
            remote_runtime.selection_policy,
            events_tx.clone(),
            scope_registry.clone(),
            worker_health.clone(),
        )
        .map_err(|err| io::Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?,
    );
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                observability,
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
    use crate::api::ListThreadsResponse;
    use crate::api::ThreadResponse;
    use crate::config::GatewayConfig;
    use crate::config::GatewayRemoteRuntimeConfig;
    use crate::config::GatewayRemoteSelectionPolicy;
    use crate::config::GatewayRemoteWorkerConfig;
    use crate::config::GatewayRuntimeMode;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_arg0::Arg0DispatchPaths;
    use codex_core::config::Config;
    use codex_core::config_loader::LoaderOverrides;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;
    use tokio::net::TcpListener;
    use tokio::time::Duration;
    use tokio::time::sleep;
    use tokio::time::timeout;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
    use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

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
    async fn remote_runtime_round_robins_threads_across_workers() {
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
        assert_eq!(health.remote_workers, None);

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
            "name": null,
            "turns": [],
        })
    }

    async fn expect_remote_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
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

    async fn read_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> JSONRPCMessage {
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

    async fn write_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        message: JSONRPCMessage,
    ) {
        websocket
            .send(Message::Text(
                serde_json::to_string(&message)
                    .expect("message should serialize")
                    .into(),
            ))
            .await
            .expect("message should send");
    }
}
