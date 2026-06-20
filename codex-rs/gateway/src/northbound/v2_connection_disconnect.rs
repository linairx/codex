use crate::northbound::v2::STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2_connection::GatewayV2ConnectionClose;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::northbound::v2_connection::WorkerServerRequestCleanupReport;
use crate::northbound::v2_server_request_cleanup::resolve_server_requests_for_worker;
use crate::northbound::v2_wire::downstream_protocol_violation_reason;
use crate::northbound::v2_wire::log_downstream_protocol_violation;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use std::io;

pub(crate) async fn handle_disconnected_app_server_event(
    socket: &mut WebSocket,
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    event_state: &mut GatewayV2EventState,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    message: &str,
) -> io::Result<Option<GatewayV2ConnectionClose>> {
    let protocol_violation_reason = downstream_protocol_violation_reason(message);
    if let Some(reason) = protocol_violation_reason {
        log_downstream_protocol_violation(
            connection.request_context,
            worker_id,
            worker_websocket_url,
            reason,
            message,
            downstream.worker_count(),
            event_state,
        );
        connection
            .observability
            .record_v2_worker_protocol_violation(worker_id, "downstream", reason);
        send_observed_close_frame(
            socket,
            connection.observability,
            connection.request_context,
            close_code::ERROR,
            &format!("downstream app-server disconnected: {message}"),
            connection.client_send_timeout,
        )
        .await?;
        return Ok(Some(GatewayV2ConnectionClose {
            outcome: "downstream_protocol_violation",
            reject_pending_server_requests: true,
        }));
    }

    let mut cleanup = WorkerServerRequestCleanup::default();
    if (downstream.reconnect_state.is_some() || !downstream.single_worker())
        && downstream.remove_worker(worker_id)
    {
        cleanup = resolve_server_requests_for_worker(
            socket,
            connection,
            &mut event_state.pending_server_requests,
            &mut event_state.resolved_server_requests,
            worker_id,
            WorkerServerRequestCleanupReport {
                worker_websocket_url,
                remaining_worker_count: downstream.worker_count(),
                disconnect_message: Some(message),
                message: "downstream worker disconnected within shared gateway v2 session",
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
        cleanup = resolve_server_requests_for_worker(
            socket,
            connection,
            &mut event_state.pending_server_requests,
            &mut event_state.resolved_server_requests,
            worker_id,
            WorkerServerRequestCleanupReport {
                worker_websocket_url,
                remaining_worker_count: 0,
                disconnect_message: Some(message),
                message: "downstream app-server disconnected with unresolved gateway server requests",
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
        &format!("downstream app-server disconnected: {message}"),
        connection.client_send_timeout,
    )
    .await?;
    Ok(Some(GatewayV2ConnectionClose {
        outcome: if protocol_violation_reason.is_some() {
            "downstream_protocol_violation"
        } else {
            "downstream_session_ended"
        },
        reject_pending_server_requests: false,
    }))
}
