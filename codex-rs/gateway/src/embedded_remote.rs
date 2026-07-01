use super::GatewayServer;
use super::REMOTE_WORKER_RECONNECT_DELAY;
use super::gateway_v2_transport_config;
use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::api::GatewayExecutionMode;
use crate::config::GatewayConfig;
use crate::config::GatewayRemoteRuntimeConfig;
use crate::config::GatewayRemoteWorkerConfig;
use crate::config::normalize_remote_account_id;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_runtime::RemoteWorkerGatewayRuntime;
use crate::remote_runtime::RemoteWorkerRuntimeState;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::AppServerGatewayRuntime;
use crate::runtime::GatewayRuntime;
use crate::runtime::GatewayRuntimeHealthConfig;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use crate::worker_pool::GatewayWorkerPoolState;
use axum::serve;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_client::RemoteAppServerEndpoint;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;

#[path = "embedded_remote_loop.rs"]
mod embedded_remote_loop;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct UnlabeledRemoteAccountWorker {
    pub(super) worker_id: usize,
    pub(super) websocket_url: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct RemoteAccountLabelSummary {
    pub(super) complete: bool,
    pub(super) unlabeled_workers: Vec<UnlabeledRemoteAccountWorker>,
}

impl RemoteAccountLabelSummary {
    pub(super) fn unlabeled_worker_count(&self) -> usize {
        self.unlabeled_workers.len()
    }
}

pub(crate) use embedded_remote_loop::run_event_loop;
use embedded_remote_loop::run_remote_worker_loop;

pub(super) fn remote_connect_args(
    remote_runtime: &GatewayRemoteWorkerConfig,
    gateway_config: &GatewayConfig,
) -> RemoteAppServerConnectArgs {
    RemoteAppServerConnectArgs {
        endpoint: RemoteAppServerEndpoint::WebSocket {
            websocket_url: remote_runtime.websocket_url.clone(),
            auth_token: remote_runtime.auth_token.clone(),
        },
        client_name: gateway_config.client_name.clone(),
        client_version: gateway_config.client_version.clone(),
        experimental_api: gateway_config.experimental_api,
        mcp_server_openai_form_elicitation: false,
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

pub(super) fn unlabeled_remote_account_workers(
    remote_runtime: &GatewayRemoteRuntimeConfig,
) -> Vec<UnlabeledRemoteAccountWorker> {
    if remote_runtime.workers.len() <= 1 {
        return Vec::new();
    }

    remote_runtime
        .workers
        .iter()
        .enumerate()
        .filter(|(_, worker)| {
            worker
                .account_id
                .as_deref()
                .map(str::trim)
                .is_none_or(str::is_empty)
        })
        .map(|(worker_id, worker)| UnlabeledRemoteAccountWorker {
            worker_id,
            websocket_url: worker.websocket_url.clone(),
        })
        .collect()
}

pub(super) fn remote_account_label_summary(
    remote_runtime: &GatewayRemoteRuntimeConfig,
) -> RemoteAccountLabelSummary {
    let unlabeled_workers = unlabeled_remote_account_workers(remote_runtime);
    RemoteAccountLabelSummary {
        complete: unlabeled_workers.is_empty(),
        unlabeled_workers,
    }
}

pub(super) fn warn_unlabeled_remote_account_workers(
    remote_runtime: &GatewayRemoteRuntimeConfig,
    account_label_summary: &RemoteAccountLabelSummary,
) {
    let unlabeled_account_workers = &account_label_summary.unlabeled_workers;
    if unlabeled_account_workers.is_empty() {
        return;
    }

    let unlabeled_worker_ids: Vec<usize> = unlabeled_account_workers
        .iter()
        .map(|worker| worker.worker_id)
        .collect();
    let unlabeled_worker_websocket_urls: Vec<&str> = unlabeled_account_workers
        .iter()
        .map(|worker| worker.websocket_url.as_str())
        .collect();
    tracing::warn!(
        total_worker_count = remote_runtime.workers.len(),
        unlabeled_worker_count = unlabeled_account_workers.len(),
        unlabeled_worker_ids = ?unlabeled_worker_ids,
        unlabeled_worker_websocket_urls = ?unlabeled_worker_websocket_urls,
        unlabeled_account_workers = ?unlabeled_account_workers,
        "gateway remote multi-worker account labels are incomplete; configure --remote-account-id for every worker before validating account-aware routing"
    );
}

pub(super) fn log_complete_remote_account_workers(
    remote_runtime: &GatewayRemoteRuntimeConfig,
    account_label_summary: &RemoteAccountLabelSummary,
) {
    if remote_runtime.workers.len() <= 1 || !account_label_summary.complete {
        return;
    }

    let labeled_worker_ids: Vec<usize> = remote_runtime
        .workers
        .iter()
        .enumerate()
        .map(|(worker_id, _)| worker_id)
        .collect();
    let labeled_worker_websocket_urls: Vec<&str> = remote_runtime
        .workers
        .iter()
        .map(|worker| worker.websocket_url.as_str())
        .collect();
    let labeled_worker_account_ids: Vec<&str> = remote_runtime
        .workers
        .iter()
        .map(|worker| worker.account_id.as_deref().unwrap_or_default())
        .collect();

    tracing::info!(
        total_worker_count = remote_runtime.workers.len(),
        labeled_worker_ids = ?labeled_worker_ids,
        labeled_worker_websocket_urls = ?labeled_worker_websocket_urls,
        labeled_worker_account_ids = ?labeled_worker_account_ids,
        "gateway remote multi-worker account labels are complete"
    );
}

pub(super) fn record_remote_account_worker_label_metrics(
    observability: &GatewayObservability,
    remote_runtime: &GatewayRemoteRuntimeConfig,
) {
    if remote_runtime.workers.len() <= 1 {
        return;
    }

    for (worker_id, worker) in remote_runtime.workers.iter().enumerate() {
        let event = if worker
            .account_id
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            "unlabeled"
        } else {
            "labeled"
        };
        observability.record_remote_account_label_event(worker_id, event);
    }
}

pub(super) async fn start_remote_runtime_gateway_http_server(
    gateway_config: GatewayConfig,
    otel: Option<codex_otel::OtelProvider>,
    initialize_response: codex_app_server_protocol::InitializeResponse,
) -> io::Result<GatewayServer> {
    let remote_runtime = gateway_config
        .remote_runtime
        .as_ref()
        .ok_or_else(missing_remote_runtime_config)?;
    if remote_runtime.workers.is_empty() {
        return Err(missing_remote_runtime_config());
    }
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: remote_runtime.selection_policy,
        workers: remote_runtime
            .workers
            .iter()
            .map(|worker| GatewayRemoteWorkerConfig {
                websocket_url: worker.websocket_url.trim().to_string(),
                auth_token: worker.auth_token.clone(),
                account_id: normalize_remote_account_id(worker.account_id.clone()),
            })
            .collect(),
    };
    let account_label_summary = remote_account_label_summary(&remote_runtime);
    warn_unlabeled_remote_account_workers(&remote_runtime, &account_label_summary);
    log_complete_remote_account_workers(&remote_runtime, &account_label_summary);

    let (events_tx, _events_rx) =
        tokio::sync::broadcast::channel(gateway_config.event_buffer_capacity);
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let v2_transport = gateway_v2_transport_config(&gateway_config);
    let observability =
        GatewayObservability::from_otel(otel.as_ref(), gateway_config.audit_logs_enabled)
            .with_operator_events(events_tx.clone());
    record_remote_account_worker_label_metrics(&observability, &remote_runtime);
    let v2_connection_health = observability.v2_connection_health();
    let worker_pool = Arc::new(GatewayWorkerPoolState::from_remote_runtime(&remote_runtime));
    observability.record_worker_pool_inventory(&worker_pool.snapshot());
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(
        remote_runtime
            .workers
            .iter()
            .map(|worker| (worker.websocket_url.clone(), worker.account_id.clone()))
            .collect(),
    ));
    let admission = GatewayAdmissionController::new(GatewayAdmissionConfig {
        request_rate_limit_per_minute: gateway_config.request_rate_limit_per_minute,
        turn_start_quota_per_minute: gateway_config.turn_start_quota_per_minute,
    });
    let mut workers = Vec::with_capacity(remote_runtime.workers.len());
    let mut app_servers = Vec::with_capacity(remote_runtime.workers.len());
    let mut remaining_workers: Vec<usize> = (0..remote_runtime.workers.len()).collect();
    while !remaining_workers.is_empty() {
        let mut connected_workers = Vec::new();
        for worker_id in remaining_workers.iter().copied() {
            let worker = &remote_runtime.workers[worker_id];
            match timeout(
                Duration::from_secs(1),
                GatewayRemoteWorker::connect(
                    worker_id,
                    remote_connect_args(worker, &gateway_config),
                ),
            )
            .await
            {
                Ok(Ok((managed_worker, app_server))) => {
                    worker_health.mark_healthy(worker_id);
                    connected_workers.push(worker_id);
                    workers.push(managed_worker);
                    app_servers.push(app_server);
                }
                Ok(Err(err)) => {
                    worker_health.mark_reconnecting(
                        worker_id,
                        Some(format!(
                            "failed to connect remote worker {worker_id} at `{}`: {err}",
                            worker.websocket_url
                        )),
                        REMOTE_WORKER_RECONNECT_DELAY,
                    );
                    tracing::warn!(
                        worker_id,
                        websocket_url = worker.websocket_url.as_str(),
                        retry_delay_seconds = REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                        %err,
                        "gateway remote worker connect attempt failed"
                    );
                }
                Err(_) => {
                    worker_health.mark_reconnecting(
                        worker_id,
                        Some(format!(
                            "gateway remote worker {worker_id} at `{}` timed out while connecting",
                            worker.websocket_url
                        )),
                        REMOTE_WORKER_RECONNECT_DELAY,
                    );
                    tracing::warn!(
                        worker_id,
                        websocket_url = worker.websocket_url.as_str(),
                        retry_delay_seconds = REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                        "gateway remote worker connect attempt timed out"
                    );
                }
            }
        }

        for worker_id in connected_workers {
            if let Some(index) = remaining_workers
                .iter()
                .position(|remaining_worker_id| *remaining_worker_id == worker_id)
            {
                remaining_workers.remove(index);
            }
        }

        if !remaining_workers.is_empty() {
            sleep(REMOTE_WORKER_RECONNECT_DELAY).await;
        }
    }
    let v2_session_factory = if remote_runtime.workers.len() == 1 {
        Some(Arc::new(
            GatewayV2SessionFactory::remote_single_with_account_id(
                remote_connect_args(&remote_runtime.workers[0], &gateway_config),
                initialize_response,
                remote_runtime.workers[0].account_id.clone(),
            )
            .with_worker_health(worker_health.clone()),
        ))
    } else {
        Some(Arc::new(
            GatewayV2SessionFactory::remote_multi_with_account_ids(
                remote_runtime
                    .workers
                    .iter()
                    .map(|worker| remote_connect_args(worker, &gateway_config))
                    .collect(),
                initialize_response,
                remote_runtime
                    .workers
                    .iter()
                    .map(|worker| worker.account_id.clone())
                    .collect(),
            )
            .with_worker_health(worker_health.clone()),
        ))
    };
    let runtime: Arc<dyn GatewayRuntime> = Arc::new(
        RemoteWorkerGatewayRuntime::new(
            workers.clone(),
            remote_runtime.selection_policy,
            events_tx.clone(),
            scope_registry.clone(),
            RemoteWorkerRuntimeState {
                worker_health: worker_health.clone(),
                worker_pool: worker_pool.clone(),
            },
            v2_transport,
            observability.clone(),
        )
        .map_err(|err| io::Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?,
    );
    let listener = TcpListener::bind(gateway_config.bind_address).await?;
    let local_addr = listener.local_addr()?;
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel::<()>();
    let http_scope_registry = scope_registry.clone();
    let http_observability = observability.clone();
    let serve_task = tokio::spawn(async move {
        serve(
            listener,
            crate::northbound::http::router_with_observability(
                runtime,
                gateway_config.auth.clone(),
                admission,
                http_observability,
                http_scope_registry,
                v2_session_factory,
                crate::northbound::v2::GatewayV2Timeouts {
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

    let mut event_shutdown_txs = Vec::with_capacity(app_servers.len());
    let mut event_tasks = Vec::with_capacity(app_servers.len());
    for (worker, app_server) in workers.into_iter().zip(app_servers) {
        let runtime = AppServerGatewayRuntime::new_with_worker_id(
            worker.request_handle_arc(),
            Some(worker.id()),
            GatewayExecutionMode::WorkerManaged,
            events_tx.clone(),
            scope_registry.clone(),
            GatewayRuntimeHealthConfig {
                remote_worker_health: Some(worker_health.clone()),
                worker_pool: Some(worker_pool.clone()),
                v2_transport,
                v2_connection_health: v2_connection_health.clone(),
                observability: observability.clone(),
            },
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
