use super::REMOTE_WORKER_RECONNECT_DELAY;
use crate::api::GatewayServerRequest;
use crate::event::GatewayEvent;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::AppServerGatewayRuntime;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::AppServerEvent;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ServerNotification;
use std::io;
use tokio::sync::oneshot;
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteWorkerLoopExit {
    Shutdown,
    Disconnected { message: String },
}

pub(crate) async fn run_event_loop(
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

pub(super) async fn run_remote_worker_loop(
    worker: GatewayRemoteWorker,
    app_server: codex_app_server_client::RemoteAppServerClient,
    runtime: AppServerGatewayRuntime,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> io::Result<()> {
    let mut app_server = AppServerClient::Remote(app_server);
    loop {
        match run_remote_worker_session(app_server, &runtime, &mut shutdown_rx).await? {
            RemoteWorkerLoopExit::Shutdown => break,
            RemoteWorkerLoopExit::Disconnected { message } => {
                runtime.publish_event(GatewayEvent::reconnecting(
                    worker.id(),
                    worker.websocket_url().to_string(),
                    message,
                    REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                ));
                let mut reconnected_client = None;
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        _ = sleep(REMOTE_WORKER_RECONNECT_DELAY) => {
                            match worker.reconnect().await {
                                Ok(client) => {
                                    tracing::info!(
                                        worker_id = worker.id(),
                                        websocket_url = worker.websocket_url(),
                                        "gateway remote worker reconnected"
                                    );
                                    runtime.mark_worker_healthy();
                                    runtime.publish_event(GatewayEvent::reconnected(
                                        worker.id(),
                                        worker.websocket_url().to_string(),
                                    ));
                                    reconnected_client = Some(AppServerClient::Remote(client));
                                    break;
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        worker_id = worker.id(),
                                        websocket_url = worker.websocket_url(),
                                        retry_delay_seconds = REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                                        %err,
                                        "gateway remote worker reconnect attempt failed"
                                    );
                                    runtime.mark_worker_reconnecting(
                                        Some(format!(
                                            "failed to reconnect remote worker {} at `{}`: {err}",
                                            worker.id(),
                                            worker.websocket_url(),
                                        )),
                                        REMOTE_WORKER_RECONNECT_DELAY,
                                    );
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
                    tracing::warn!(
                        worker_id = runtime.worker_id(),
                        retry_delay_seconds = REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                        "gateway remote worker event stream ended; entering reconnect loop"
                    );
                    let message = "remote app server event stream ended".to_string();
                    runtime.mark_worker_reconnecting(
                        Some(message.clone()),
                        REMOTE_WORKER_RECONNECT_DELAY,
                    );
                    runtime.clear_owned_pending_server_requests();
                    app_server.shutdown().await?;
                    return Ok(RemoteWorkerLoopExit::Disconnected { message });
                };
                let disconnected_message = match &event {
                    AppServerEvent::Disconnected { message } => Some(message.clone()),
                    _ => None,
                };
                handle_app_server_event(event, &app_server, runtime).await?;
                if let Some(message) = disconnected_message {
                    tracing::warn!(
                        worker_id = runtime.worker_id(),
                        retry_delay_seconds = REMOTE_WORKER_RECONNECT_DELAY.as_secs(),
                        disconnect_message = %message,
                        "gateway remote worker disconnected; entering reconnect loop"
                    );
                    runtime.mark_worker_reconnecting(
                        Some(message.clone()),
                        REMOTE_WORKER_RECONNECT_DELAY,
                    );
                    runtime.clear_owned_pending_server_requests();
                    app_server.shutdown().await?;
                    return Ok(RemoteWorkerLoopExit::Disconnected { message });
                }
            }
        }
    }
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
                                    code: super::super::UNSUPPORTED_SERVER_REQUEST_CODE,
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
                        super::super::UNSUPPORTED_SERVER_REQUEST_MESSAGE,
                    ));
                    app_server
                        .reject_server_request(
                            request.id().clone(),
                            JSONRPCErrorError {
                                code: super::super::UNSUPPORTED_SERVER_REQUEST_CODE,
                                message: super::super::UNSUPPORTED_SERVER_REQUEST_MESSAGE
                                    .to_string(),
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
