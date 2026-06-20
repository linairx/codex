use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_wire::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::observe_client_response_send_failure;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use axum::extract::ws::CloseFrame;
use axum::extract::ws::Message as WebSocketMessage;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use std::future::Future;
use std::io;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::time::timeout;
use tracing::warn;

pub(crate) const MAX_CLOSE_REASON_BYTES: usize = 123;

pub(crate) fn websocket_close_reason(reason: &str) -> &str {
    if reason.len() <= MAX_CLOSE_REASON_BYTES {
        return reason;
    }

    let mut end = MAX_CLOSE_REASON_BYTES;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    &reason[..end]
}

pub(crate) async fn send_websocket_message(
    socket: &mut WebSocket,
    message: WebSocketMessage,
    client_send_timeout: Duration,
) -> io::Result<()> {
    await_io_with_timeout(
        async { socket.send(message).await.map_err(io::Error::other) },
        client_send_timeout,
        "gateway websocket send timed out",
    )
    .await
}

pub(crate) async fn send_jsonrpc(
    socket: &mut WebSocket,
    message: JSONRPCMessage,
    client_send_timeout: Duration,
) -> io::Result<()> {
    let payload = serde_json::to_string(&message).map_err(io::Error::other)?;
    send_websocket_message(
        socket,
        WebSocketMessage::Text(payload.into()),
        client_send_timeout,
    )
    .await
}

pub(crate) async fn send_jsonrpc_error(
    socket: &mut WebSocket,
    id: RequestId,
    error: JSONRPCErrorError,
    client_send_timeout: Duration,
) -> io::Result<()> {
    send_jsonrpc(
        socket,
        JSONRPCMessage::Error(JSONRPCError { id, error }),
        client_send_timeout,
    )
    .await
}

pub(crate) async fn send_close_frame(
    socket: &mut WebSocket,
    code: u16,
    reason: &str,
    client_send_timeout: Duration,
) -> io::Result<()> {
    send_websocket_message(
        socket,
        WebSocketMessage::Close(Some(CloseFrame {
            code,
            reason: websocket_close_reason(reason).to_string().into(),
        })),
        client_send_timeout,
    )
    .await
}

pub(crate) async fn send_client_jsonrpc(
    socket: &mut WebSocket,
    connection: &GatewayV2ConnectionContext<'_>,
    request_id: &RequestId,
    method: &str,
    message: JSONRPCMessage,
) -> io::Result<()> {
    send_jsonrpc(socket, message, connection.client_send_timeout)
        .await
        .inspect_err(|err| {
            let outcome = classify_v2_connection_error(err);
            observe_client_response_send_failure(
                connection.observability,
                connection.request_context,
                request_id,
                method,
                outcome,
                err,
            );
        })
}

pub(crate) async fn send_client_jsonrpc_error(
    socket: &mut WebSocket,
    connection: &GatewayV2ConnectionContext<'_>,
    request_id: &RequestId,
    method: &str,
    error: JSONRPCErrorError,
) -> io::Result<()> {
    send_client_jsonrpc(
        socket,
        connection,
        request_id,
        method,
        JSONRPCMessage::Error(JSONRPCError {
            id: request_id.clone(),
            error,
        }),
    )
    .await
}

pub(crate) async fn send_observed_close_frame(
    socket: &mut WebSocket,
    observability: &GatewayObservability,
    request_context: &GatewayRequestContext,
    code: u16,
    reason: &str,
    client_send_timeout: Duration,
) -> io::Result<()> {
    send_close_frame(socket, code, reason, client_send_timeout)
        .await
        .inspect_err(|err| {
            let outcome = classify_v2_connection_error(err);
            observability.record_v2_close_frame_send_failure(code, outcome);
            log_close_frame_send_failure(request_context, code, reason, outcome, err);
        })
}

pub(crate) async fn send_observed_invalid_payload_close(
    socket: &mut WebSocket,
    observability: &GatewayObservability,
    request_context: &GatewayRequestContext,
    err: &io::Error,
    client_send_timeout: Duration,
) -> io::Result<()> {
    if err.kind() == ErrorKind::InvalidData {
        let code = if err
            .to_string()
            .starts_with(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
        {
            close_code::INVALID
        } else {
            close_code::PROTOCOL
        };
        send_observed_close_frame(
            socket,
            observability,
            request_context,
            code,
            &err.to_string(),
            client_send_timeout,
        )
        .await
    } else {
        Ok(())
    }
}

pub(crate) fn log_close_frame_send_failure(
    request_context: &GatewayRequestContext,
    code: u16,
    reason: &str,
    outcome: &str,
    err: &io::Error,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        code,
        reason = websocket_close_reason(reason),
        outcome,
        error = %err,
        "failed to deliver gateway v2 close frame to northbound client"
    );
}

pub(crate) async fn await_io_with_timeout<T>(
    future: impl Future<Output = io::Result<T>>,
    timeout_duration: Duration,
    timeout_message: &'static str,
) -> io::Result<T> {
    timeout(timeout_duration, future)
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, timeout_message))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn format_lagged_close_reason_reports_skipped_event_count() {
        assert_eq!(
            crate::northbound::v2_wire::format_lagged_close_reason(3),
            "downstream app-server event stream lagged: skipped 3 events"
        );
    }

    #[test]
    fn websocket_close_reason_truncates_to_protocol_limit() {
        let reason = "x".repeat(200);
        assert_eq!(websocket_close_reason(&reason).len(), 123);
    }
}
