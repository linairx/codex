use crate::error::GatewayError;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
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

pub(crate) async fn await_io_with_timeout<T>(
    future: impl Future<Output = io::Result<T>>,
    timeout_duration: Duration,
    timeout_message: &'static str,
) -> io::Result<T> {
    timeout(timeout_duration, future)
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, timeout_message))?
}
