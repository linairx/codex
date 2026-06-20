use crate::northbound::v2::STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2_account_capacity::sync_worker_account_capacity_from_notification;
use crate::northbound::v2_connection::GatewayV2ConnectionClose;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::northbound::v2_connection::WorkerServerRequestCleanupReport;
use crate::northbound::v2_connection_disconnect::handle_disconnected_app_server_event;
use crate::northbound::v2_connection_lifecycle::log_downstream_backpressure_close;
use crate::northbound::v2_connection_server_request::handle_server_request_event;
use crate::northbound::v2_notifications::forwarded_connection_notification_duplicate;
use crate::northbound::v2_notifications::log_suppressed_duplicate_connection_notification;
use crate::northbound::v2_notifications::log_suppressed_hidden_thread_notification;
use crate::northbound::v2_notifications::log_suppressed_opted_out_notification;
use crate::northbound::v2_notifications::log_suppressed_skills_changed_notification;
use crate::northbound::v2_notifications::record_forwarded_connection_notification;
use crate::northbound::v2_notifications::should_deduplicate_connection_notification;
use crate::northbound::v2_scope::notification_visible_to;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::format_lagged_close_reason;
use crate::northbound::v2_wire::log_notification_send_failure;
use crate::northbound::v2_wire::server_notification_to_jsonrpc;
use crate::northbound::v2_wire_send::send_jsonrpc;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use crate::v2::GatewayV2SessionFactory;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_client::AppServerEvent;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use std::collections::HashMap;
use std::io;

pub(crate) async fn handle_app_server_event(
    socket: &mut WebSocket,
    downstream: &mut GatewayV2DownstreamRouter,
    session_factory: &GatewayV2SessionFactory,
    connection: &GatewayV2ConnectionContext<'_>,
    event_state: &mut GatewayV2EventState,
    _pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
    downstream_event: crate::northbound::v2_connection::DownstreamWorkerEvent,
) -> io::Result<Option<GatewayV2ConnectionClose>> {
    let worker_id = downstream_event.worker_id;
    if worker_id.is_some() && !downstream.has_worker(worker_id) && downstream.worker_count() > 0 {
        return Ok(None);
    }
    let worker_websocket_url = downstream
        .websocket_url_for_worker_id(worker_id)
        .to_string();
    let Some(event) = downstream_event.event else {
        let mut cleanup = WorkerServerRequestCleanup::default();
        if (downstream.reconnect_state.is_some() || !downstream.single_worker())
            && downstream.remove_worker(worker_id)
        {
            cleanup =
                crate::northbound::v2_server_request_cleanup::resolve_server_requests_for_worker(
                    socket,
                    connection,
                    &mut event_state.pending_server_requests,
                    &mut event_state.resolved_server_requests,
                    worker_id,
                    WorkerServerRequestCleanupReport {
                        worker_websocket_url: worker_websocket_url.as_str(),
                        remaining_worker_count: downstream.worker_count(),
                        disconnect_message: None,
                        message: "downstream worker session ended within shared gateway v2 session",
                    },
                )
                .await?;
            if cleanup.has_stranded_connection_scoped_requests() {
                send_observed_close_frame(
                    socket,
                    connection.observability,
                    connection.request_context,
                    close_code::ERROR,
                    STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON,
                    connection.client_send_timeout,
                )
                .await?;
                return Ok(Some(GatewayV2ConnectionClose {
                    outcome: "stranded_connection_scoped_server_request",
                    reject_pending_server_requests: true,
                }));
            }
            if downstream.reconnect_state.is_some() {
                downstream
                    .reconnect_missing_workers_after_disconnect(connection.observability)
                    .await;
            }
            if downstream.reconnect_state.is_some() && downstream.worker_count() > 0 {
                return Ok(None);
            }
            if downstream.worker_count() > 0 {
                return Ok(None);
            }
        } else if worker_id.is_none() {
            cleanup = crate::northbound::v2_server_request_cleanup::resolve_server_requests_for_worker(
                socket,
                connection,
                &mut event_state.pending_server_requests,
                &mut event_state.resolved_server_requests,
                worker_id,
                WorkerServerRequestCleanupReport {
                    worker_websocket_url: worker_websocket_url.as_str(),
                    remaining_worker_count: 0,
                    disconnect_message: None,
                    message: "downstream app-server session ended with unresolved gateway server requests",
                },
            )
            .await?;
        }
        if cleanup.has_stranded_connection_scoped_requests() {
            send_observed_close_frame(
                socket,
                connection.observability,
                connection.request_context,
                close_code::ERROR,
                STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON,
                connection.client_send_timeout,
            )
            .await?;
            return Ok(Some(GatewayV2ConnectionClose {
                outcome: "stranded_connection_scoped_server_request",
                reject_pending_server_requests: true,
            }));
        }
        send_observed_close_frame(
            socket,
            connection.observability,
            connection.request_context,
            close_code::ERROR,
            crate::northbound::v2::DOWNSTREAM_SESSION_ENDED_CLOSE_REASON,
            connection.client_send_timeout,
        )
        .await?;
        return Ok(Some(GatewayV2ConnectionClose {
            outcome: "downstream_session_ended",
            reject_pending_server_requests: false,
        }));
    };
    match event {
        AppServerEvent::Lagged { skipped } => {
            log_downstream_backpressure_close(
                connection.request_context,
                worker_id,
                worker_websocket_url.as_str(),
                skipped,
                &event_state.pending_server_requests,
                &event_state.resolved_server_requests,
            );
            connection
                .observability
                .record_v2_downstream_backpressure(worker_id);
            send_observed_close_frame(
                socket,
                connection.observability,
                connection.request_context,
                close_code::POLICY,
                &format_lagged_close_reason(skipped),
                connection.client_send_timeout,
            )
            .await?;
            return Ok(Some(GatewayV2ConnectionClose {
                outcome: "downstream_backpressure",
                reject_pending_server_requests: true,
            }));
        }
        AppServerEvent::ServerNotification(notification) => {
            sync_worker_account_capacity_from_notification(
                downstream,
                connection.request_context,
                connection.observability,
                worker_id,
                &notification,
            );
            let Some(notification) = server_notification_to_jsonrpc(
                notification,
                connection.request_context,
                connection.observability,
                worker_id,
                worker_websocket_url.as_str(),
                &mut event_state.resolved_server_requests,
            )?
            else {
                return Ok(None);
            };
            if connection
                .opt_out_notification_methods
                .contains(notification.method.as_str())
            {
                log_suppressed_opted_out_notification(
                    connection.request_context,
                    worker_id,
                    worker_websocket_url.as_str(),
                    &notification,
                );
                connection
                    .observability
                    .record_v2_suppressed_notification(&notification.method, "opted_out");
                return Ok(None);
            }
            if downstream.multi_worker_topology()
                && notification.method == "skills/changed"
                && event_state.skills_changed_pending_refresh
            {
                log_suppressed_skills_changed_notification(
                    connection.request_context,
                    worker_id,
                    worker_websocket_url.as_str(),
                    &notification,
                );
                connection
                    .observability
                    .record_v2_suppressed_notification(&notification.method, "pending_refresh");
                return Ok(None);
            }
            if downstream.multi_worker_topology()
                && should_deduplicate_connection_notification(&notification)
                && let Some(forwarded_notification) = forwarded_connection_notification_duplicate(
                    &event_state.forwarded_connection_notifications,
                    worker_id,
                    &notification,
                )
            {
                log_suppressed_duplicate_connection_notification(
                    connection.request_context,
                    worker_id,
                    worker_websocket_url.as_str(),
                    forwarded_notification.worker_id,
                    downstream.websocket_url_for_worker_id(forwarded_notification.worker_id),
                    &notification,
                );
                connection
                    .observability
                    .record_v2_suppressed_notification(&notification.method, "duplicate");
                return Ok(None);
            }
            if notification_visible_to(
                connection.scope_registry,
                connection.request_context,
                &notification,
            ) {
                if downstream.multi_worker_topology() && notification.method == "skills/changed" {
                    event_state.skills_changed_pending_refresh = true;
                }
                if downstream.multi_worker_topology()
                    && should_deduplicate_connection_notification(&notification)
                {
                    record_forwarded_connection_notification(
                        &mut event_state.forwarded_connection_notifications,
                        worker_id,
                        &notification,
                    );
                }
                let method = notification.method.clone();
                if let Err(err) = send_jsonrpc(
                    socket,
                    JSONRPCMessage::Notification(notification),
                    connection.client_send_timeout,
                )
                .await
                {
                    let outcome = classify_v2_connection_error(&err);
                    log_notification_send_failure(
                        connection.request_context,
                        worker_id,
                        worker_websocket_url.as_str(),
                        &method,
                        outcome,
                        &err,
                    );
                    connection
                        .observability
                        .record_v2_notification_send_failure(&method, outcome);
                    return Err(err);
                }
                connection
                    .observability
                    .record_v2_forwarded_notification(&method);
            } else {
                log_suppressed_hidden_thread_notification(
                    connection.request_context,
                    worker_id,
                    worker_websocket_url.as_str(),
                    &notification,
                );
                connection
                    .observability
                    .record_v2_suppressed_notification(&notification.method, "hidden_thread");
            }
        }
        AppServerEvent::ServerRequest(request) => {
            handle_server_request_event(
                socket,
                downstream,
                session_factory,
                connection,
                event_state,
                worker_id,
                request,
            )
            .await?;
        }
        AppServerEvent::Disconnected { message } => {
            return handle_disconnected_app_server_event(
                socket,
                downstream,
                connection,
                event_state,
                worker_id,
                worker_websocket_url.as_str(),
                message.as_str(),
            )
            .await;
        }
    }

    Ok(None)
}
