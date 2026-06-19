//! Wire-level helpers for northbound v2 transport handling.
//!
//! This module owns JSON-RPC conversion, close-reason handling, and transport
//! send-failure logging so `v2.rs` can stay focused on routing and session
//! orchestration.

use crate::error::GatewayError;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_server_requests::log_dropped_duplicate_resolved_server_request;
use crate::northbound::v2_server_requests::pending_server_request_log_fields;
use crate::northbound::v2_server_requests::resolved_server_request_log_fields;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use axum::extract::ws::CloseFrame;
use axum::extract::ws::Message as WebSocketMessage;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use std::future::Future;
use std::io;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::time::timeout;
use tracing::warn;

pub(crate) const DOWNSTREAM_BACKPRESSURE_CLOSE_REASON: &str =
    "downstream app-server event stream lagged";
pub(crate) const INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON: &str =
    "invalid gateway websocket JSON-RPC payload";
pub(crate) const INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON: &str =
    "invalid gateway websocket UTF-8 payload";
pub(crate) const MAX_CLOSE_REASON_BYTES: usize = 123;

pub(crate) fn gateway_error_to_jsonrpc_error(error: GatewayError) -> JSONRPCErrorError {
    match error {
        GatewayError::InvalidRequest(message) | GatewayError::NotFound(message) => {
            JSONRPCErrorError {
                code: -32602,
                message,
                data: None,
            }
        }
        GatewayError::RateLimited {
            message,
            retry_after_seconds,
        } => JSONRPCErrorError {
            code: -32001,
            message,
            data: Some(json!({
                "retryAfterSeconds": retry_after_seconds,
            })),
        },
        GatewayError::Upstream(message) => JSONRPCErrorError {
            code: -32603,
            message,
            data: None,
        },
    }
}

pub(crate) fn hidden_thread_error_message(request: &JSONRPCRequest) -> &str {
    let _ = request;
    "thread not found"
}

pub(crate) fn gateway_error_outcome(error: &GatewayError) -> &'static str {
    match error {
        GatewayError::InvalidRequest(_) | GatewayError::NotFound(_) => "invalid_params",
        GatewayError::RateLimited { .. } => "rate_limited",
        GatewayError::Upstream(_) => "internal_error",
    }
}

pub(crate) fn jsonrpc_error_outcome_code(code: i64) -> &'static str {
    match code {
        -32001 => "rate_limited",
        -32600 => "invalid_request",
        -32602 => "invalid_params",
        _ => "jsonrpc_error",
    }
}

pub(crate) fn observe_v2_request(
    observability: &GatewayObservability,
    context: &GatewayRequestContext,
    method: &str,
    outcome: &str,
    duration: std::time::Duration,
) {
    observability.record_v2_request(method, outcome, duration);
    observability.emit_v2_audit_log(method, outcome, duration, context);
}

pub(crate) fn observe_v2_connection(
    observability: &GatewayObservability,
    connection_id: u64,
    context: &GatewayRequestContext,
    outcome: &str,
    detail: Option<&str>,
    pending_counts: GatewayV2ConnectionPendingCounts,
    duration: std::time::Duration,
) {
    let completion_counts = observability
        .v2_connection_health()
        .mark_connection_completed(
            connection_id,
            outcome,
            detail,
            duration,
            pending_counts.clone(),
        );
    observability.record_v2_connection(
        outcome,
        duration,
        pending_counts.clone(),
        completion_counts.max_pending_client_request_count,
        completion_counts.max_server_request_backlog_count,
    );
    observability.emit_v2_connection_audit_log(
        outcome,
        duration,
        context,
        detail,
        pending_counts.clone(),
        &completion_counts,
    );
    observability.emit_v2_connection_log(
        outcome,
        duration,
        context,
        detail,
        pending_counts,
        &completion_counts,
    );
}

pub(crate) fn classify_v2_connection_error(err: &io::Error) -> &'static str {
    match err.kind() {
        ErrorKind::InvalidData => "protocol_violation",
        ErrorKind::TimedOut => "client_send_timed_out",
        ErrorKind::BrokenPipe
        | ErrorKind::ConnectionAborted
        | ErrorKind::ConnectionReset
        | ErrorKind::UnexpectedEof => "client_disconnected",
        _ => "connection_error",
    }
}

pub(crate) fn jsonrpc_request_to_client_request(
    request: JSONRPCRequest,
) -> io::Result<ClientRequest> {
    tagged_message_to_type("request", request)
}

pub(crate) fn jsonrpc_notification_to_client_notification(
    notification: JSONRPCNotification,
) -> io::Result<ClientNotification> {
    tagged_message_to_type("notification", notification)
}

pub(crate) fn parse_client_jsonrpc_text(text: &str) -> io::Result<JSONRPCMessage> {
    serde_json::from_str::<JSONRPCMessage>(text).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("{INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON}: {err}"),
        )
    })
}

pub(crate) fn parse_client_jsonrpc_binary(bytes: &[u8]) -> io::Result<JSONRPCMessage> {
    let text = std::str::from_utf8(bytes).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("{INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON}: {err}"),
        )
    })?;
    parse_client_jsonrpc_text(text)
}

pub(crate) fn protocol_violation_reason_from_invalid_payload(err: &io::Error) -> &'static str {
    let message = err.to_string();
    if message.starts_with(INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON) {
        "invalid_utf8"
    } else {
        "invalid_jsonrpc"
    }
}

pub(crate) fn downstream_protocol_violation_reason(message: &str) -> Option<&'static str> {
    if message.contains("sent invalid JSON-RPC")
        || message.contains("sent invalid initialize response")
        || message.contains("sent unexpected JSON-RPC response id")
        || message.contains("sent unexpected JSON-RPC error id")
    {
        Some("invalid_jsonrpc")
    } else if message.contains("sent non-text JSON-RPC frame")
        || message.contains("sent non-text initialize frame")
    {
        Some("invalid_binary")
    } else {
        None
    }
}

pub(crate) fn log_downstream_connect_protocol_violation(
    request_context: &GatewayRequestContext,
    reason: &str,
    message: &str,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        reason,
        downstream_message = message,
        "downstream app-server sent a malformed v2 protocol frame during initialize"
    );
}

pub(crate) fn log_downstream_reconnect_protocol_violation(
    request_context: &GatewayRequestContext,
    worker_id: usize,
    worker_websocket_url: &str,
    reason: &str,
    message: &str,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id,
        worker_websocket_url,
        reason,
        downstream_message = message,
        "downstream app-server sent a malformed v2 protocol frame during worker reconnect"
    );
}

pub(crate) fn log_downstream_protocol_violation(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    reason: &str,
    message: &str,
    active_worker_count: usize,
    event_state: &GatewayV2EventState,
) {
    let pending_log_fields =
        pending_server_request_log_fields(&event_state.pending_server_requests);
    let resolved_log_fields =
        resolved_server_request_log_fields(&event_state.resolved_server_requests);

    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        reason,
        downstream_message = message,
        active_worker_count,
        pending_server_request_count = pending_log_fields.gateway_request_ids.len(),
        pending_server_request_ids = ?pending_log_fields.gateway_request_ids,
        pending_downstream_server_request_ids = ?pending_log_fields.downstream_request_ids,
        pending_server_request_methods = ?pending_log_fields.methods,
        pending_thread_ids = ?pending_log_fields.thread_ids,
        pending_worker_ids = ?pending_log_fields.worker_ids,
        pending_worker_websocket_urls = ?pending_log_fields.worker_websocket_urls,
        answered_but_unresolved_server_request_count = resolved_log_fields.gateway_request_ids.len(),
        answered_but_unresolved_gateway_request_ids = ?resolved_log_fields.gateway_request_ids,
        answered_but_unresolved_downstream_request_ids = ?resolved_log_fields.downstream_request_ids,
        answered_but_unresolved_server_request_methods = ?resolved_log_fields.methods,
        answered_but_unresolved_thread_ids = ?resolved_log_fields.thread_ids,
        answered_but_unresolved_worker_ids = ?resolved_log_fields.worker_ids,
        answered_but_unresolved_worker_websocket_urls =
            ?resolved_log_fields.worker_websocket_urls,
        "downstream app-server sent a malformed v2 protocol frame"
    );
}

pub(crate) fn log_notification_send_failure(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    method: &str,
    outcome: &str,
    err: &io::Error,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        method,
        outcome,
        error = %err,
        "failed to deliver downstream notification to northbound v2 client"
    );
}

pub(crate) fn log_downstream_server_request_forward_failure(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    method: &str,
    outcome: &str,
    err: &io::Error,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        method,
        outcome,
        error = %err,
        "failed to deliver downstream server request to northbound v2 client"
    );
}

pub(crate) fn log_client_response_send_failure(
    request_context: &GatewayRequestContext,
    request_id: &RequestId,
    method: &str,
    outcome: &str,
    err: &io::Error,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        request_id = ?request_id,
        method,
        outcome,
        error = %err,
        "failed to deliver gateway v2 client request response to northbound client"
    );
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

pub(crate) fn observe_client_response_send_failure(
    observability: &GatewayObservability,
    request_context: &GatewayRequestContext,
    request_id: &RequestId,
    method: &str,
    outcome: &str,
    err: &io::Error,
) {
    observability.record_v2_client_response_send_failure(method, outcome);
    log_client_response_send_failure(request_context, request_id, method, outcome, err);
}

pub(crate) fn server_request_to_jsonrpc(
    request: ServerRequest,
    gateway_request_id: RequestId,
) -> io::Result<(JSONRPCRequest, RequestId)> {
    let value = serde_json::to_value(request).map_err(io::Error::other)?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "server request is missing method"))?
        .to_string();
    let downstream_request_id =
        serde_json::from_value::<RequestId>(value.get("id").cloned().ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidData, "server request is missing id")
        })?)
        .map_err(io::Error::other)?;
    let params = value.get("params").cloned();

    Ok((
        JSONRPCRequest {
            id: gateway_request_id,
            method,
            params,
            trace: None,
        },
        downstream_request_id,
    ))
}

pub(crate) fn request_params<T: DeserializeOwned>(request: &JSONRPCRequest) -> io::Result<T> {
    serde_json::from_value(request.params.clone().unwrap_or(Value::Null)).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid request params for `{}`: {err}", request.method),
        )
    })
}

pub(crate) fn tagged_message_to_type<T: DeserializeOwned + Serialize>(
    kind: &str,
    message: impl Serialize,
) -> io::Result<T> {
    serde_json::from_value(serde_json::to_value(message).map_err(io::Error::other)?).map_err(
        |err| {
            io::Error::new(
                ErrorKind::InvalidInput,
                format!("invalid client {kind} payload: {err}"),
            )
        },
    )
}

pub(crate) fn tagged_type_to_notification<T: Serialize>(
    message: T,
) -> io::Result<JSONRPCNotification> {
    let value = serde_json::to_value(message).map_err(io::Error::other)?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "notification is missing method"))?
        .to_string();
    let params = value.get("params").cloned();
    Ok(JSONRPCNotification { method, params })
}

pub(crate) fn server_notification_to_jsonrpc(
    notification: ServerNotification,
    request_context: &GatewayRequestContext,
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    resolved_server_requests: &mut std::collections::HashMap<
        DownstreamServerRequestKey,
        ResolvedServerRequestRoute,
    >,
) -> io::Result<Option<JSONRPCNotification>> {
    let mut notification = tagged_type_to_notification(notification)?;
    if notification.method == "serverRequest/resolved"
        && let Some(params) = notification.params.as_mut()
        && let Some(request_id_value) = params.get("requestId").cloned()
    {
        let downstream_request_id =
            serde_json::from_value::<RequestId>(request_id_value).map_err(io::Error::other)?;
        if let Some(route) = resolved_server_requests.remove(&DownstreamServerRequestKey {
            worker_id,
            request_id: downstream_request_id.clone(),
        }) {
            params["requestId"] =
                serde_json::to_value(route.gateway_request_id).map_err(io::Error::other)?;
            observability.record_v2_server_request_lifecycle_event(
                "downstream_server_request_resolved",
                "serverRequest/resolved",
            );
        } else if worker_id.is_some() {
            log_dropped_duplicate_resolved_server_request(
                request_context,
                worker_id,
                worker_websocket_url,
                &downstream_request_id,
                resolved_server_requests,
            );
            observability.record_v2_server_request_lifecycle_event(
                "duplicate_resolved_replay",
                "serverRequest/resolved",
            );
            return Ok(None);
        }
    }
    Ok(Some(notification))
}

pub(crate) fn format_lagged_close_reason(skipped: usize) -> String {
    format!("{DOWNSTREAM_BACKPRESSURE_CLOSE_REASON}: skipped {skipped} events")
}

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
    use pretty_assertions::assert_eq;

    #[test]
    fn format_lagged_close_reason_reports_skipped_event_count() {
        assert_eq!(
            super::format_lagged_close_reason(3),
            "downstream app-server event stream lagged: skipped 3 events"
        );
    }

    #[test]
    fn websocket_close_reason_truncates_to_protocol_limit() {
        let reason = "x".repeat(200);
        assert_eq!(super::websocket_close_reason(&reason).len(), 123);
    }
}
