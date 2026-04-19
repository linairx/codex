use crate::admission::GatewayAdmissionController;
use crate::auth::GatewayAuth;
use crate::auth::GatewayAuthError;
use crate::error::GatewayError;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::CloseFrame;
use axum::extract::ws::Message as WebSocketMessage;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::header::ORIGIN;
use axum::response::IntoResponse;
use codex_app_server_client::AppServerClient;
use codex_app_server_client::AppServerEvent;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tracing::warn;

const INVALID_REQUEST_CODE: i64 = -32600;
const INVALID_PARAMS_CODE: i64 = -32602;
const INTERNAL_ERROR_CODE: i64 = -32603;
const RATE_LIMITED_ERROR_CODE: i64 = -32001;
const DOWNSTREAM_SESSION_ENDED_CLOSE_REASON: &str = "downstream app-server session ended";
const INITIALIZE_TIMEOUT_CLOSE_REASON: &str = "initialize request timed out";
const DOWNSTREAM_BACKPRESSURE_CLOSE_REASON: &str = "downstream app-server event stream lagged";
const INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON: &str =
    "invalid gateway websocket JSON-RPC payload";
const INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON: &str = "invalid gateway websocket UTF-8 payload";
const TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE: &str =
    "too many pending server requests for websocket connection";
const MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION: usize = 64;
const MAX_CLOSE_REASON_BYTES: usize = 123;

#[derive(Clone, Copy, Debug)]
pub struct GatewayV2Timeouts {
    pub initialize: Duration,
    pub client_send: Duration,
    pub max_pending_server_requests: usize,
}

impl Default for GatewayV2Timeouts {
    fn default() -> Self {
        Self {
            initialize: Duration::from_secs(30),
            client_send: Duration::from_secs(10),
            max_pending_server_requests: MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
        }
    }
}

#[derive(Clone)]
pub struct GatewayV2State {
    pub auth: GatewayAuth,
    pub admission: GatewayAdmissionController,
    pub observability: GatewayObservability,
    pub scope_registry: Arc<GatewayScopeRegistry>,
    pub session_factory: Option<Arc<GatewayV2SessionFactory>>,
    pub timeouts: GatewayV2Timeouts,
}

struct GatewayV2ConnectionContext<'a> {
    admission: &'a GatewayAdmissionController,
    observability: &'a GatewayObservability,
    scope_registry: &'a GatewayScopeRegistry,
    request_context: &'a GatewayRequestContext,
    client_send_timeout: Duration,
    max_pending_server_requests: usize,
}

pub async fn websocket_upgrade_handler(
    websocket: WebSocketUpgrade,
    State(state): State<GatewayV2State>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if headers.contains_key(ORIGIN) {
        return (
            StatusCode::FORBIDDEN,
            "gateway websocket connections must not include an Origin header",
        )
            .into_response();
    }

    if !is_authorized(&state.auth, &headers) {
        return GatewayAuthError.into_response();
    }

    let context = match GatewayRequestContext::from_headers(&headers) {
        Ok(context) => context,
        Err(err) => return err.into_response(),
    };

    let Some(session_factory) = state.session_factory else {
        return (
            StatusCode::NOT_IMPLEMENTED,
            "gateway app-server v2 compatibility is not enabled for this runtime",
        )
            .into_response();
    };

    websocket
        .on_upgrade(move |socket| async move {
            if let Err(err) = run_websocket_connection(
                socket,
                session_factory,
                state.admission,
                state.observability,
                state.scope_registry,
                context,
                state.timeouts,
            )
            .await
            {
                warn!(%err, "gateway v2 websocket connection failed");
            }
        })
        .into_response()
}

fn is_authorized(auth: &GatewayAuth, headers: &HeaderMap) -> bool {
    match auth {
        GatewayAuth::Disabled => true,
        GatewayAuth::BearerToken { token } => headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == format!("Bearer {token}")),
    }
}

async fn run_websocket_connection(
    mut socket: WebSocket,
    session_factory: Arc<GatewayV2SessionFactory>,
    admission: GatewayAdmissionController,
    observability: GatewayObservability,
    scope_registry: Arc<GatewayScopeRegistry>,
    context: GatewayRequestContext,
    timeouts: GatewayV2Timeouts,
) -> io::Result<()> {
    let initialize_started_at = Instant::now();
    let initialize_request = match recv_initialize_request(&mut socket, timeouts).await {
        Ok(request) => request,
        Err(err) if err.kind() == ErrorKind::TimedOut => {
            send_close_frame(
                &mut socket,
                close_code::POLICY,
                INITIALIZE_TIMEOUT_CLOSE_REASON,
                timeouts.client_send,
            )
            .await?;
            observe_v2_request(
                &observability,
                &context,
                "initialize",
                "timed_out",
                initialize_started_at.elapsed(),
            );
            return Ok(());
        }
        Err(err) if err.kind() == ErrorKind::InvalidData => {
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let initialize_request_id = initialize_request.id.clone();
    let initialize_params = match request_params::<InitializeParams>(&initialize_request) {
        Ok(params) => params,
        Err(err) => {
            send_jsonrpc_error(
                &mut socket,
                initialize_request_id,
                JSONRPCErrorError {
                    code: INVALID_REQUEST_CODE,
                    message: err.to_string(),
                    data: None,
                },
                timeouts.client_send,
            )
            .await?;
            observe_v2_request(
                &observability,
                &context,
                "initialize",
                "invalid_request",
                initialize_started_at.elapsed(),
            );
            return Ok(());
        }
    };
    let mut app_server = match session_factory.connect(&initialize_params).await {
        Ok(app_server) => app_server,
        Err(err) => {
            send_jsonrpc_error(
                &mut socket,
                initialize_request_id,
                JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("gateway failed to connect downstream app-server: {err}"),
                    data: None,
                },
                timeouts.client_send,
            )
            .await?;
            observe_v2_request(
                &observability,
                &context,
                "initialize",
                "internal_error",
                initialize_started_at.elapsed(),
            );
            return Ok(());
        }
    };
    send_jsonrpc(
        &mut socket,
        JSONRPCMessage::Response(JSONRPCResponse {
            id: initialize_request.id,
            result: serde_json::to_value(session_factory.initialize_response())
                .map_err(io::Error::other)?,
        }),
        timeouts.client_send,
    )
    .await?;
    observe_v2_request(
        &observability,
        &context,
        "initialize",
        "ok",
        initialize_started_at.elapsed(),
    );

    let mut pending_server_requests = HashSet::<RequestId>::new();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: timeouts.client_send,
        max_pending_server_requests: timeouts.max_pending_server_requests,
    };
    loop {
        tokio::select! {
            frame = socket.recv() => {
                let Some(frame) = frame else {
                    break;
                };
                match frame {
                    Ok(WebSocketMessage::Text(text)) => {
                        let message = match parse_client_jsonrpc_text(&text) {
                            Ok(message) => message,
                            Err(err) => {
                                send_invalid_payload_close(
                                    &mut socket,
                                    &err,
                                    timeouts.client_send,
                                )
                                .await?;
                                break;
                            }
                        };
                        handle_client_message(
                            &mut socket,
                            &app_server,
                            &connection,
                            &mut pending_server_requests,
                            message,
                        )
                        .await?;
                    }
                    Ok(WebSocketMessage::Binary(bytes)) => {
                        let message = match parse_client_jsonrpc_binary(&bytes) {
                            Ok(message) => message,
                            Err(err) => {
                                send_invalid_payload_close(
                                    &mut socket,
                                    &err,
                                    timeouts.client_send,
                                )
                                .await?;
                                break;
                            }
                        };
                        handle_client_message(
                            &mut socket,
                            &app_server,
                            &connection,
                            &mut pending_server_requests,
                            message,
                        )
                        .await?;
                    }
                    Ok(WebSocketMessage::Close(_)) => break,
                    Ok(WebSocketMessage::Ping(payload)) => {
                        send_websocket_message(
                            &mut socket,
                            WebSocketMessage::Pong(payload),
                            timeouts.client_send,
                        )
                        .await?;
                    }
                    Ok(WebSocketMessage::Pong(_)) => {}
                    Err(err) => return Err(io::Error::other(format!("gateway websocket receive failed: {err}"))),
                }
            }
            event = app_server.next_event() => {
                let Some(event) = event else {
                    send_close_frame(
                        &mut socket,
                        close_code::ERROR,
                        DOWNSTREAM_SESSION_ENDED_CLOSE_REASON,
                        timeouts.client_send,
                    )
                    .await?;
                    break;
                };
                let should_close = handle_app_server_event(
                    &mut socket,
                    &app_server,
                    &connection,
                    &mut pending_server_requests,
                    event,
                )
                .await?;
                if should_close {
                    break;
                }
            }
        }
    }

    app_server.shutdown().await
}

async fn recv_initialize_request(
    socket: &mut WebSocket,
    timeouts: GatewayV2Timeouts,
) -> io::Result<JSONRPCRequest> {
    tokio::time::timeout(timeouts.initialize, async {
        loop {
            let Some(frame) = socket.recv().await else {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "gateway websocket connection closed before initialize",
                ));
            };
            match frame {
                Ok(WebSocketMessage::Text(text)) => {
                    let message = match parse_client_jsonrpc_text(&text) {
                        Ok(message) => message,
                        Err(err) => {
                            send_invalid_payload_close(socket, &err, timeouts.client_send).await?;
                            return Err(err);
                        }
                    };
                    if let Some(request) =
                        handle_pre_initialize_message(socket, message, timeouts.client_send).await?
                    {
                        return Ok(request);
                    }
                }
                Ok(WebSocketMessage::Binary(bytes)) => {
                    let message = match parse_client_jsonrpc_binary(&bytes) {
                        Ok(message) => message,
                        Err(err) => {
                            send_invalid_payload_close(socket, &err, timeouts.client_send).await?;
                            return Err(err);
                        }
                    };
                    if let Some(request) =
                        handle_pre_initialize_message(socket, message, timeouts.client_send).await?
                    {
                        return Ok(request);
                    }
                }
                Ok(WebSocketMessage::Ping(payload)) => {
                    send_websocket_message(
                        socket,
                        WebSocketMessage::Pong(payload),
                        timeouts.client_send,
                    )
                    .await?;
                }
                Ok(WebSocketMessage::Pong(_)) => {}
                Ok(WebSocketMessage::Close(_)) => {
                    return Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "gateway websocket connection closed before initialize",
                    ));
                }
                Err(err) => {
                    return Err(io::Error::other(format!(
                        "gateway websocket receive failed during initialize: {err}"
                    )));
                }
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(ErrorKind::TimedOut, INITIALIZE_TIMEOUT_CLOSE_REASON))?
}

fn parse_client_jsonrpc_text(text: &str) -> io::Result<JSONRPCMessage> {
    serde_json::from_str::<JSONRPCMessage>(text).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("{INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON}: {err}"),
        )
    })
}

fn parse_client_jsonrpc_binary(bytes: &[u8]) -> io::Result<JSONRPCMessage> {
    let text = std::str::from_utf8(bytes).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("{INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON}: {err}"),
        )
    })?;
    parse_client_jsonrpc_text(text)
}

async fn handle_pre_initialize_message(
    socket: &mut WebSocket,
    message: JSONRPCMessage,
    client_send_timeout: Duration,
) -> io::Result<Option<JSONRPCRequest>> {
    match message {
        JSONRPCMessage::Request(request) if request.method == "initialize" => Ok(Some(request)),
        JSONRPCMessage::Request(request) => {
            send_jsonrpc_error(
                socket,
                request.id,
                JSONRPCErrorError {
                    code: INVALID_REQUEST_CODE,
                    message: "initialize must be the first request".to_string(),
                    data: None,
                },
                client_send_timeout,
            )
            .await?;
            Ok(None)
        }
        JSONRPCMessage::Notification(_)
        | JSONRPCMessage::Response(_)
        | JSONRPCMessage::Error(_) => Ok(None),
    }
}

async fn handle_client_message(
    socket: &mut WebSocket,
    app_server: &AppServerClient,
    connection: &GatewayV2ConnectionContext<'_>,
    pending_server_requests: &mut HashSet<RequestId>,
    message: JSONRPCMessage,
) -> io::Result<()> {
    match message {
        JSONRPCMessage::Request(request) => {
            let started_at = Instant::now();
            let method = request.method.clone();
            if request.method == "initialize" {
                send_jsonrpc_error(
                    socket,
                    request.id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_CODE,
                        message: "connection is already initialized".to_string(),
                        data: None,
                    },
                    connection.client_send_timeout,
                )
                .await?;
                observe_v2_request(
                    connection.observability,
                    connection.request_context,
                    &method,
                    "invalid_request",
                    started_at.elapsed(),
                );
                return Ok(());
            }

            if let Err(err) = connection
                .admission
                .check_request(connection.request_context, request.method.as_str())
            {
                let outcome = gateway_error_outcome(&err);
                send_jsonrpc_error(
                    socket,
                    request.id,
                    gateway_error_to_jsonrpc_error(err),
                    connection.client_send_timeout,
                )
                .await?;
                observe_v2_request(
                    connection.observability,
                    connection.request_context,
                    &method,
                    outcome,
                    started_at.elapsed(),
                );
                return Ok(());
            }

            if let Err(err) = enforce_request_scope(
                connection.scope_registry,
                connection.request_context,
                &request,
            ) {
                let outcome = gateway_error_outcome(&err);
                send_jsonrpc_error(
                    socket,
                    request.id,
                    gateway_error_to_jsonrpc_error(err),
                    connection.client_send_timeout,
                )
                .await?;
                observe_v2_request(
                    connection.observability,
                    connection.request_context,
                    &method,
                    outcome,
                    started_at.elapsed(),
                );
                return Ok(());
            }

            let request_id = request.id.clone();
            let client_request = jsonrpc_request_to_client_request(request)?;
            match app_server.request(client_request).await {
                Ok(Ok(result)) => {
                    let result = apply_response_scope_policy(
                        connection.scope_registry,
                        connection.request_context,
                        &method,
                        result,
                    )?;
                    send_jsonrpc(
                        socket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request_id,
                            result,
                        }),
                        connection.client_send_timeout,
                    )
                    .await?;
                    observe_v2_request(
                        connection.observability,
                        connection.request_context,
                        &method,
                        "ok",
                        started_at.elapsed(),
                    );
                }
                Ok(Err(error)) => {
                    let outcome = jsonrpc_error_outcome_code(error.code);
                    send_jsonrpc_error(socket, request_id, error, connection.client_send_timeout)
                        .await?;
                    observe_v2_request(
                        connection.observability,
                        connection.request_context,
                        &method,
                        outcome,
                        started_at.elapsed(),
                    );
                }
                Err(err) => {
                    send_jsonrpc_error(
                        socket,
                        request_id,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("gateway upstream request failed: {err}"),
                            data: None,
                        },
                        connection.client_send_timeout,
                    )
                    .await?;
                    observe_v2_request(
                        connection.observability,
                        connection.request_context,
                        &method,
                        "internal_error",
                        started_at.elapsed(),
                    );
                }
            }
        }
        JSONRPCMessage::Notification(notification) => {
            if notification.method == "initialized" {
                return Ok(());
            }
            let client_notification = jsonrpc_notification_to_client_notification(notification)?;
            app_server.notify(client_notification).await?;
        }
        JSONRPCMessage::Response(response) => {
            if pending_server_requests.remove(&response.id) {
                app_server
                    .resolve_server_request(response.id, response.result)
                    .await?;
            }
        }
        JSONRPCMessage::Error(error) => {
            if pending_server_requests.remove(&error.id) {
                app_server
                    .reject_server_request(error.id, error.error)
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_app_server_event(
    socket: &mut WebSocket,
    app_server: &AppServerClient,
    connection: &GatewayV2ConnectionContext<'_>,
    pending_server_requests: &mut HashSet<RequestId>,
    event: AppServerEvent,
) -> io::Result<bool> {
    match event {
        AppServerEvent::Lagged { skipped } => {
            send_close_frame(
                socket,
                close_code::POLICY,
                &format_lagged_close_reason(skipped),
                connection.client_send_timeout,
            )
            .await?;
            return Ok(true);
        }
        AppServerEvent::ServerNotification(notification) => {
            let notification = server_notification_to_jsonrpc(notification)?;
            if notification_visible_to(
                connection.scope_registry,
                connection.request_context,
                &notification,
            ) {
                send_jsonrpc(
                    socket,
                    JSONRPCMessage::Notification(notification),
                    connection.client_send_timeout,
                )
                .await?;
            }
        }
        AppServerEvent::ServerRequest(request) => {
            let request = server_request_to_jsonrpc(request)?;
            if request_visible_to(
                connection.scope_registry,
                connection.request_context,
                &request,
            ) {
                if let Some(error) = pending_server_request_limit_error(
                    pending_server_requests.len(),
                    connection.max_pending_server_requests,
                ) {
                    warn!(
                        pending_server_request_count = pending_server_requests.len(),
                        limit = connection.max_pending_server_requests,
                        request_id = ?request.id,
                        method = request.method,
                        "rejecting downstream server request because the gateway websocket connection is saturated"
                    );
                    app_server.reject_server_request(request.id, error).await?;
                    return Ok(false);
                }
                pending_server_requests.insert(request.id.clone());
                send_jsonrpc(
                    socket,
                    JSONRPCMessage::Request(request),
                    connection.client_send_timeout,
                )
                .await?;
            } else {
                let message = hidden_thread_error_message(&request).to_string();
                app_server
                    .reject_server_request(
                        request.id,
                        JSONRPCErrorError {
                            code: INVALID_PARAMS_CODE,
                            message,
                            data: None,
                        },
                    )
                    .await?;
            }
        }
        AppServerEvent::Disconnected { message } => {
            send_close_frame(
                socket,
                close_code::ERROR,
                &format!("downstream app-server disconnected: {message}"),
                connection.client_send_timeout,
            )
            .await?;
            return Ok(true);
        }
    }

    Ok(false)
}

fn enforce_request_scope(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: &JSONRPCRequest,
) -> Result<(), GatewayError> {
    match request.method.as_str() {
        "thread/resume" => {
            if has_non_null_param(request.params.as_ref(), "history")
                || has_non_null_param(request.params.as_ref(), "path")
            {
                return Err(GatewayError::InvalidRequest(
                    "gateway scope policy requires `thread/resume` to use `threadId` only"
                        .to_string(),
                ));
            }
        }
        "thread/fork" => {
            if has_non_null_param(request.params.as_ref(), "path") {
                return Err(GatewayError::InvalidRequest(
                    "gateway scope policy requires `thread/fork` to use `threadId` only"
                        .to_string(),
                ));
            }
        }
        _ => {}
    }

    if let Some(thread_id) = request_thread_id(request)
        && !scope_registry.thread_visible_to(context, thread_id)
    {
        return Err(GatewayError::NotFound(format!(
            "thread not found: {thread_id}"
        )));
    }

    Ok(())
}

fn apply_response_scope_policy(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    method: &str,
    mut result: Value,
) -> io::Result<Value> {
    match method {
        "thread/start" | "thread/resume" | "thread/fork" => {
            if let Some(thread_id) = response_thread_id(&result) {
                scope_registry.register_thread(thread_id.to_string(), context.clone());
            }
        }
        "review/start" => {
            if let Some(review_thread_id) = result.get("reviewThreadId").and_then(Value::as_str) {
                scope_registry.register_thread(review_thread_id.to_string(), context.clone());
            }
        }
        "thread/list" => {
            let data = result
                .get_mut("data")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        "thread/list response is missing data array",
                    )
                })?;
            data.retain(|thread| {
                thread
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|thread_id| scope_registry.thread_visible_to(context, thread_id))
            });
        }
        "thread/loaded/list" => {
            let data = result
                .get_mut("data")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        "thread/loaded/list response is missing data array",
                    )
                })?;
            data.retain(|thread_id| {
                thread_id
                    .as_str()
                    .is_some_and(|thread_id| scope_registry.thread_visible_to(context, thread_id))
            });
        }
        _ => {}
    }

    Ok(result)
}

fn pending_server_request_limit_error(
    pending_server_request_count: usize,
    max_pending_server_requests: usize,
) -> Option<JSONRPCErrorError> {
    if pending_server_request_count < max_pending_server_requests {
        None
    } else {
        Some(JSONRPCErrorError {
            code: RATE_LIMITED_ERROR_CODE,
            message: TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE.to_string(),
            data: None,
        })
    }
}

fn notification_visible_to(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    notification: &JSONRPCNotification,
) -> bool {
    notification
        .params
        .as_ref()
        .and_then(notification_thread_id)
        .is_none_or(|thread_id| scope_registry.thread_visible_to(context, thread_id))
}

fn request_visible_to(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: &JSONRPCRequest,
) -> bool {
    request_thread_id(request)
        .is_none_or(|thread_id| scope_registry.thread_visible_to(context, thread_id))
}

fn request_thread_id(request: &JSONRPCRequest) -> Option<&str> {
    request.params.as_ref().and_then(param_thread_id)
}

fn format_lagged_close_reason(skipped: usize) -> String {
    format!("{DOWNSTREAM_BACKPRESSURE_CLOSE_REASON}: skipped {skipped} events")
}

fn websocket_close_reason(reason: &str) -> &str {
    if reason.len() <= MAX_CLOSE_REASON_BYTES {
        return reason;
    }

    let mut end = MAX_CLOSE_REASON_BYTES;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    &reason[..end]
}

fn param_thread_id(params: &Value) -> Option<&str> {
    params.get("threadId").and_then(Value::as_str)
}

fn has_non_null_param(params: Option<&Value>, name: &str) -> bool {
    params
        .and_then(|params| params.get(name))
        .is_some_and(|value| !value.is_null())
}

fn response_thread_id(result: &Value) -> Option<&str> {
    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
}

fn notification_thread_id(params: &Value) -> Option<&str> {
    params
        .get("threadId")
        .and_then(Value::as_str)
        .or_else(|| {
            params
                .get("thread")
                .and_then(|thread| thread.get("id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            params
                .get("turn")
                .and_then(|turn| turn.get("threadId"))
                .and_then(Value::as_str)
        })
}

fn gateway_error_to_jsonrpc_error(error: GatewayError) -> JSONRPCErrorError {
    match error {
        GatewayError::InvalidRequest(message) | GatewayError::NotFound(message) => {
            JSONRPCErrorError {
                code: INVALID_PARAMS_CODE,
                message,
                data: None,
            }
        }
        GatewayError::RateLimited {
            message,
            retry_after_seconds,
        } => JSONRPCErrorError {
            code: RATE_LIMITED_ERROR_CODE,
            message,
            data: Some(json!({
                "retryAfterSeconds": retry_after_seconds,
            })),
        },
        GatewayError::Upstream(message) => JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message,
            data: None,
        },
    }
}

fn hidden_thread_error_message(request: &JSONRPCRequest) -> &str {
    let _ = request;
    "thread not found"
}

fn gateway_error_outcome(error: &GatewayError) -> &'static str {
    match error {
        GatewayError::InvalidRequest(_) | GatewayError::NotFound(_) => "invalid_params",
        GatewayError::RateLimited { .. } => "rate_limited",
        GatewayError::Upstream(_) => "internal_error",
    }
}

fn jsonrpc_error_outcome_code(code: i64) -> &'static str {
    match code {
        RATE_LIMITED_ERROR_CODE => "rate_limited",
        INVALID_REQUEST_CODE => "invalid_request",
        INVALID_PARAMS_CODE => "invalid_params",
        _ => "jsonrpc_error",
    }
}

fn observe_v2_request(
    observability: &GatewayObservability,
    context: &GatewayRequestContext,
    method: &str,
    outcome: &str,
    duration: std::time::Duration,
) {
    observability.record_v2_request(method, outcome, duration);
    observability.emit_v2_audit_log(method, outcome, duration, context);
}

fn jsonrpc_request_to_client_request(request: JSONRPCRequest) -> io::Result<ClientRequest> {
    tagged_message_to_type("request", request)
}

fn jsonrpc_notification_to_client_notification(
    notification: JSONRPCNotification,
) -> io::Result<ClientNotification> {
    tagged_message_to_type("notification", notification)
}

fn server_notification_to_jsonrpc(
    notification: ServerNotification,
) -> io::Result<JSONRPCNotification> {
    tagged_type_to_notification(notification)
}

fn server_request_to_jsonrpc(request: ServerRequest) -> io::Result<JSONRPCRequest> {
    let value = serde_json::to_value(request).map_err(io::Error::other)?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "server request is missing method"))?
        .to_string();
    let id =
        serde_json::from_value::<RequestId>(value.get("id").cloned().ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidData, "server request is missing id")
        })?)
        .map_err(io::Error::other)?;
    let params = value.get("params").cloned();

    Ok(JSONRPCRequest {
        id,
        method,
        params,
        trace: None,
    })
}

fn request_params<T: DeserializeOwned>(request: &JSONRPCRequest) -> io::Result<T> {
    serde_json::from_value(request.params.clone().unwrap_or(Value::Null)).map_err(|err| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid request params for `{}`: {err}", request.method),
        )
    })
}

fn tagged_message_to_type<T: DeserializeOwned + Serialize>(
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

fn tagged_type_to_notification<T: Serialize>(message: T) -> io::Result<JSONRPCNotification> {
    let value = serde_json::to_value(message).map_err(io::Error::other)?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "notification is missing method"))?
        .to_string();
    let params = value.get("params").cloned();
    Ok(JSONRPCNotification { method, params })
}

async fn await_io_with_timeout<T>(
    future: impl Future<Output = io::Result<T>>,
    timeout_duration: Duration,
    timeout_message: &'static str,
) -> io::Result<T> {
    tokio::time::timeout(timeout_duration, future)
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, timeout_message))?
}

async fn send_websocket_message(
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

async fn send_jsonrpc(
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

async fn send_jsonrpc_error(
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

async fn send_close_frame(
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

async fn send_invalid_payload_close(
    socket: &mut WebSocket,
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
        send_close_frame(socket, code, &err.to_string(), client_send_timeout).await
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayV2State;
    use super::GatewayV2Timeouts;
    use super::await_io_with_timeout;
    use super::websocket_upgrade_handler;
    use crate::admission::GatewayAdmissionConfig;
    use crate::admission::GatewayAdmissionController;
    use crate::auth::GatewayAuth;
    use crate::observability::GatewayObservability;
    use crate::scope::GatewayRequestContext;
    use crate::scope::GatewayScopeRegistry;
    use crate::v2::GatewayV2SessionFactory;
    use crate::v2::gateway_initialize_response;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::any;
    use codex_app_server_client::RemoteAppServerConnectArgs;
    use codex_app_server_protocol::ClientInfo;
    use codex_app_server_protocol::InitializeCapabilities;
    use codex_app_server_protocol::InitializeParams;
    use codex_app_server_protocol::InitializeResponse;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::RequestId;
    use codex_core::config::Config;
    use futures::SinkExt;
    use futures::StreamExt;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::MetricData;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::net::TcpListener;
    use tokio::time::Duration;
    use tokio::time::sleep;
    use tokio::time::timeout;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Error as WebSocketError;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    #[tokio::test]
    async fn await_io_with_timeout_returns_result_before_timeout() {
        let result = await_io_with_timeout(
            async { Ok::<_, std::io::Error>("ok") },
            Duration::from_millis(50),
            "timed out",
        )
        .await
        .expect("future should complete");

        assert_eq!(result, "ok");
    }

    #[tokio::test]
    async fn await_io_with_timeout_returns_timed_out_error() {
        let error = await_io_with_timeout(
            async {
                sleep(Duration::from_millis(50)).await;
                Ok::<_, std::io::Error>(())
            },
            Duration::from_millis(1),
            "timed out",
        )
        .await
        .expect_err("future should time out");

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "timed out");
    }

    #[tokio::test]
    async fn initialize_returns_jsonrpc_error_for_invalid_params() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("initialize".to_string()),
                    method: "initialize".to_string(),
                    params: Some(serde_json::json!({
                        "clientInfo": {
                            "name": 1
                        }
                    })),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("initialize request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected initialize error response");
        };
        assert_eq!(error.id, RequestId::String("initialize".to_string()));
        assert_eq!(error.error.code, super::INVALID_REQUEST_CODE);
        assert_eq!(
            error
                .error
                .message
                .contains("invalid request params for `initialize`"),
            true
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn initialize_must_be_the_first_request_for_binary_frames_too() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 8,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts {
                initialize: Duration::from_secs(30),
                client_send: Duration::from_secs(10),
                max_pending_server_requests: 1,
            },
        })
        .await;
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        websocket
            .send(Message::Binary(
                serde_json::to_vec(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({})),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("binary request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected pre-initialize error response");
        };
        assert_eq!(error.id, RequestId::String("model-list".to_string()));
        assert_eq!(error.error.code, super::INVALID_REQUEST_CODE);
        assert_eq!(error.error.message, "initialize must be the first request");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                experimental_api: true,
                opt_out_notification_methods: None,
            }),
        )
        .await;

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn initialize_returns_jsonrpc_error_when_downstream_connect_fails() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("initialize".to_string()),
                    method: "initialize".to_string(),
                    params: Some(
                        serde_json::to_value(InitializeParams {
                            client_info: ClientInfo {
                                name: "codex-tui".to_string(),
                                title: None,
                                version: "0.0.0-test".to_string(),
                            },
                            capabilities: None,
                        })
                        .expect("initialize params should serialize"),
                    ),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("initialize request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected initialize error response");
        };
        assert_eq!(error.id, RequestId::String("initialize".to_string()));
        assert_eq!(error.error.code, super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error
                .error
                .message
                .contains("gateway failed to connect downstream app-server"),
            true
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_rejects_origin_header() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "origin",
            "https://example.com".parse().expect("origin header"),
        );
        let err = connect_async(request)
            .await
            .expect_err("websocket handshake should fail");
        let response = match err {
            WebSocketError::Http(response) => response,
            other => panic!("expected HTTP handshake error, got {other:?}"),
        };

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_requires_bearer_token_when_configured() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::BearerToken {
                token: "secret-token".to_string(),
            },
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        let err = connect_async(request)
            .await
            .expect_err("websocket handshake should fail");
        let response = match err {
            WebSocketError::Http(response) => response,
            other => panic!("expected HTTP handshake error, got {other:?}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_returns_not_implemented_without_v2_runtime() {
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: None,
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        let err = connect_async(request)
            .await
            .expect_err("websocket handshake should fail");
        let response = match err {
            WebSocketError::Http(response) => response,
            other => panic!("expected HTTP handshake error, got {other:?}"),
        };

        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_closes_when_initialize_times_out() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts {
                initialize: Duration::from_millis(50),
                client_send: Duration::from_secs(10),
                max_pending_server_requests: super::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
            },
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let frame = timeout(Duration::from_secs(2), websocket.next())
            .await
            .expect("close frame should arrive")
            .expect("websocket should yield frame")
            .expect("close frame should decode");
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::POLICY
        );
        assert_eq!(close_frame.reason, super::INITIALIZE_TIMEOUT_CLOSE_REASON);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_closes_when_pre_initialize_text_is_not_jsonrpc() {
        let initialize_response = test_initialize_response().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url: "ws://127.0.0.1:1".to_string(),
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::PROTOCOL
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_closes_when_post_initialize_binary_is_not_utf8() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Binary(vec![0xff, 0xfe, 0xfd].into()))
            .await
            .expect("invalid binary payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::INVALID
        );
        assert!(
            close_frame
                .reason
                .starts_with(super::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON)
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[test]
    fn pending_server_request_limit_error_allows_counts_below_limit() {
        assert_eq!(super::pending_server_request_limit_error(0, 1), None);
        assert_eq!(super::pending_server_request_limit_error(3, 4), None);
    }

    #[test]
    fn pending_server_request_limit_error_rejects_counts_at_or_above_limit() {
        let error = super::pending_server_request_limit_error(1, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
        );

        let error = super::pending_server_request_limit_error(2, 1).expect("error");
        assert_eq!(error.code, super::RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.message,
            super::TOO_MANY_PENDING_SERVER_REQUESTS_MESSAGE
        );
    }

    #[tokio::test]
    async fn websocket_upgrade_applies_scope_headers_and_rate_limits() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-a".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
                request_rate_limit_per_minute: Some(1),
                turn_start_quota_per_minute: None,
            }),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-b".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-read".to_string()),
                    method: "thread/read".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-a",
                        "includeTurns": false
                    })),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("thread read should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected thread read error response");
        };
        assert_eq!(error.id, RequestId::String("thread-read".to_string()));
        assert_eq!(error.error.code, super::INVALID_PARAMS_CODE);
        assert_eq!(error.error.message, "thread not found: thread-a");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({})),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("model list should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected model list error response");
        };
        assert_eq!(error.id, RequestId::String("model-list".to_string()));
        assert_eq!(error.error.code, super::RATE_LIMITED_ERROR_CODE);
        let retry_after_seconds = error
            .error
            .data
            .as_ref()
            .and_then(|data| data.get("retryAfterSeconds"))
            .and_then(serde_json::Value::as_u64)
            .expect("retryAfterSeconds should be present");
        assert_eq!((59..=60).contains(&retry_after_seconds), true);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_rejects_thread_resume_history_and_path_bypass_over_jsonrpc() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-resume-history".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "history": [{}],
                    })),
                    trace: None,
                }))
                .expect("thread/resume history request should serialize")
                .into(),
            ))
            .await
            .expect("thread/resume history request should send");

        let JSONRPCMessage::Error(history_error) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected thread/resume history error response");
        };
        assert_eq!(
            history_error.id,
            RequestId::String("thread-resume-history".to_string())
        );
        assert_eq!(history_error.error.code, super::INVALID_PARAMS_CODE);
        assert_eq!(
            history_error.error.message,
            "gateway scope policy requires `thread/resume` to use `threadId` only"
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-resume-path".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "path": "/tmp/rollout.jsonl",
                    })),
                    trace: None,
                }))
                .expect("thread/resume path request should serialize")
                .into(),
            ))
            .await
            .expect("thread/resume path request should send");

        let JSONRPCMessage::Error(path_error) = read_websocket_message(&mut websocket).await else {
            panic!("expected thread/resume path error response");
        };
        assert_eq!(
            path_error.id,
            RequestId::String("thread-resume-path".to_string())
        );
        assert_eq!(path_error.error.code, super::INVALID_PARAMS_CODE);
        assert_eq!(
            path_error.error.message,
            "gateway scope policy requires `thread/resume` to use `threadId` only"
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_rejects_thread_fork_path_bypass_over_jsonrpc() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-fork-path".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "path": "/tmp/rollout.jsonl",
                    })),
                    trace: None,
                }))
                .expect("thread/fork path request should serialize")
                .into(),
            ))
            .await
            .expect("thread/fork path request should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected thread/fork path error response");
        };
        assert_eq!(error.id, RequestId::String("thread-fork-path".to_string()));
        assert_eq!(error.error.code, super::INVALID_PARAMS_CODE);
        assert_eq!(
            error.error.message,
            "gateway scope policy requires `thread/fork` to use `threadId` only"
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_emits_v2_request_metrics() {
        let metrics = codex_otel::MetricsClient::new(
            codex_otel::MetricsConfig::in_memory(
                "test",
                "codex-gateway",
                env!("CARGO_PKG_VERSION"),
                opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
            )
            .with_runtime_reader(),
        )
        .expect("metrics");
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::new(GatewayAdmissionConfig {
                request_rate_limit_per_minute: Some(0),
                turn_start_quota_per_minute: None,
            }),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({})),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("model list should send");

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected model list error response");
        };
        assert_eq!(error.id, RequestId::String("model-list".to_string()));

        let resource_metrics = metrics.snapshot().expect("snapshot");
        let metrics = resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);

        let mut saw_count = false;
        let mut saw_duration = false;
        for metric in metrics {
            match metric.name() {
                "gateway_v2_requests" => {
                    saw_count = true;
                    match metric.data() {
                        AggregatedMetrics::U64(data) => match data {
                            MetricData::Sum(sum) => {
                                let total: u64 = sum
                                    .data_points()
                                    .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                    .sum();
                                assert_eq!(total, 2);
                            }
                            _ => panic!("unexpected v2 count aggregation"),
                        },
                        _ => panic!("unexpected v2 count type"),
                    }
                }
                "gateway_v2_request_duration" => {
                    saw_duration = true;
                    match metric.data() {
                        AggregatedMetrics::F64(data) => {
                            match data {
                                MetricData::Histogram(histogram) => {
                                    let total_count: u64 =
                                    histogram.data_points().map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count).sum();
                                    assert_eq!(total_count, 2);
                                }
                                _ => panic!("unexpected v2 duration aggregation"),
                            }
                        }
                        _ => panic!("unexpected v2 duration type"),
                    }
                }
                _ => {}
            }
        }

        assert_eq!(saw_count, true);
        assert_eq!(saw_duration, true);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_closes_with_reason_when_downstream_disconnects() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_that_disconnects_after_initialize().await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let frame = wait_for_close_frame(&mut websocket).await;
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::ERROR
        );
        assert_eq!(
            close_frame
                .reason
                .starts_with("downstream app-server disconnected:"),
            true
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_account_read_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            "account/read",
            serde_json::json!({
                "refreshToken": false,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "gateway@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": false,
            }),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("account-read".to_string()),
                    method: "account/read".to_string(),
                    params: Some(serde_json::json!({
                        "refreshToken": false,
                    })),
                    trace: None,
                }))
                .expect("account read request should serialize")
                .into(),
            ))
            .await
            .expect("account read request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected account read response");
        };
        assert_eq!(response.id, RequestId::String("account-read".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "gateway@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": false,
            })
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_model_list_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            "model/list",
            serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": true,
            }),
            serde_json::json!({
                "data": [
                    {
                        "model": "gpt-5",
                        "provider": "openai",
                        "contextWindow": 272000,
                        "maxOutputTokens": 32000,
                        "supportsImages": true,
                        "supportsPromptCacheKey": true,
                        "supportsResponseSchema": true,
                        "supportsReasoningSummaries": true,
                        "supportsEncryptedReasoningContent": false,
                        "supportsReasoningEffort": true,
                        "supportsCustomToolCallInput": true,
                        "supportsParallelToolCalls": true,
                        "supportsToolChoiceRequired": true,
                        "supportsTerminalToolCall": true,
                        "supportsPreserveBackground": true,
                        "supportsMinimalEffortReasoning": true,
                        "supportsVerbosity": true,
                        "overrideRank": null,
                        "upgradeInfo": null,
                        "availabilityNux": null,
                        "displayName": "GPT-5",
                        "description": "Gateway test model",
                        "hidden": false,
                        "supportedReasoningEfforts": [
                            {
                                "reasoningEffort": "medium",
                                "description": "Balanced",
                            }
                        ],
                        "defaultReasoningEffort": "medium",
                        "inputModalities": ["text"],
                        "supportsPersonality": false,
                        "additionalSpeedTiers": [],
                        "isDefault": true,
                    }
                ],
                "nextCursor": null,
            }),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({
                        "cursor": null,
                        "limit": null,
                        "includeHidden": true,
                    })),
                    trace: None,
                }))
                .expect("model list request should serialize")
                .into(),
            ))
            .await
            .expect("model list request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected model list response");
        };
        assert_eq!(response.id, RequestId::String("model-list".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "data": [
                    {
                        "model": "gpt-5",
                        "provider": "openai",
                        "contextWindow": 272000,
                        "maxOutputTokens": 32000,
                        "supportsImages": true,
                        "supportsPromptCacheKey": true,
                        "supportsResponseSchema": true,
                        "supportsReasoningSummaries": true,
                        "supportsEncryptedReasoningContent": false,
                        "supportsReasoningEffort": true,
                        "supportsCustomToolCallInput": true,
                        "supportsParallelToolCalls": true,
                        "supportsToolChoiceRequired": true,
                        "supportsTerminalToolCall": true,
                        "supportsPreserveBackground": true,
                        "supportsMinimalEffortReasoning": true,
                        "supportsVerbosity": true,
                        "overrideRank": null,
                        "upgradeInfo": null,
                        "availabilityNux": null,
                        "displayName": "GPT-5",
                        "description": "Gateway test model",
                        "hidden": false,
                        "supportedReasoningEfforts": [
                            {
                                "reasoningEffort": "medium",
                                "description": "Balanced",
                            }
                        ],
                        "defaultReasoningEffort": "medium",
                        "inputModalities": ["text"],
                        "supportsPersonality": false,
                        "additionalSpeedTiers": [],
                        "isDefault": true,
                    }
                ],
                "nextCursor": null,
            })
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_additional_thread_control_requests() {
        let cases = vec![
            (
                "thread/unsubscribe",
                "thread-unsubscribe",
                serde_json::json!({
                    "threadId": "thread-visible",
                }),
                serde_json::json!({
                    "status": "unsubscribed",
                }),
            ),
            (
                "thread/compact/start",
                "thread-compact-start",
                serde_json::json!({
                    "threadId": "thread-visible",
                }),
                serde_json::json!({}),
            ),
            (
                "thread/shellCommand",
                "thread-shell-command",
                serde_json::json!({
                    "threadId": "thread-visible",
                    "command": "git status --short",
                }),
                serde_json::json!({}),
            ),
            (
                "thread/backgroundTerminals/clean",
                "thread-background-terminals-clean",
                serde_json::json!({
                    "threadId": "thread-visible",
                }),
                serde_json::json!({}),
            ),
            (
                "thread/rollback",
                "thread-rollback",
                serde_json::json!({
                    "threadId": "thread-visible",
                    "numTurns": 2,
                }),
                serde_json::json!({
                    "thread": {
                        "id": "thread-visible",
                        "name": "Visible thread",
                        "turns": [],
                    },
                }),
            ),
        ];

        for (method, request_id, params, result) in cases {
            let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
            let scope_registry = Arc::new(GatewayScopeRegistry::default());
            scope_registry.register_thread(
                "thread-visible".to_string(),
                GatewayRequestContext::default(),
            );
            let (addr, server_task) =
                spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

            let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
                .await
                .expect("websocket should connect");

            send_initialize(&mut websocket).await;
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                        id: RequestId::String(request_id.to_string()),
                        method: method.to_string(),
                        params: Some(params),
                        trace: None,
                    }))
                    .expect("request should serialize")
                    .into(),
                ))
                .await
                .expect("request should send");

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected response");
            };
            assert_eq!(response.id, RequestId::String(request_id.to_string()));
            assert_eq!(response.result, result);

            server_task.abort();
            let _ = server_task.await;
        }
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_turn_control_requests() {
        let cases = vec![
            (
                "turn/interrupt",
                "turn-interrupt",
                serde_json::json!({
                    "threadId": "thread-visible",
                    "turnId": "turn-active",
                }),
                serde_json::json!({}),
            ),
            (
                "turn/steer",
                "turn-steer",
                serde_json::json!({
                    "threadId": "thread-visible",
                    "input": [
                        {
                            "type": "text",
                            "text": "please continue with more detail",
                            "text_elements": [],
                        }
                    ],
                    "responsesapiClientMetadata": null,
                    "expectedTurnId": "turn-active",
                }),
                serde_json::json!({
                    "turnId": "turn-active",
                }),
            ),
        ];

        for (method, request_id, params, result) in cases {
            let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
            let scope_registry = Arc::new(GatewayScopeRegistry::default());
            scope_registry.register_thread(
                "thread-visible".to_string(),
                GatewayRequestContext::default(),
            );
            let (addr, server_task) =
                spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

            let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
                .await
                .expect("websocket should connect");

            send_initialize(&mut websocket).await;
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                        id: RequestId::String(request_id.to_string()),
                        method: method.to_string(),
                        params: Some(params),
                        trace: None,
                    }))
                    .expect("request should serialize")
                    .into(),
                ))
                .await
                .expect("request should send");

            let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
            else {
                panic!("expected response");
            };
            assert_eq!(response.id, RequestId::String(request_id.to_string()));
            assert_eq!(response.result, result);

            server_task.abort();
            let _ = server_task.await;
        }
    }

    #[tokio::test]
    async fn websocket_upgrade_registers_review_thread_scope_after_review_start() {
        let websocket_url = start_mock_remote_server_for_review_start_then_thread_read().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("review-start".to_string()),
                    method: "review/start".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "target": {
                            "type": "custom",
                            "instructions": "Review the current change",
                        },
                        "delivery": "detached",
                    })),
                    trace: None,
                }))
                .expect("review request should serialize")
                .into(),
            ))
            .await
            .expect("review request should send");

        let JSONRPCMessage::Response(review_response) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected review/start response");
        };
        assert_eq!(
            review_response.id,
            RequestId::String("review-start".to_string())
        );
        assert_eq!(
            review_response.result,
            serde_json::json!({
                "turn": {
                    "id": "turn-review",
                    "items": [],
                    "status": "pending",
                    "error": null,
                    "startedAt": 1,
                    "completedAt": null,
                    "durationMs": null,
                },
                "reviewThreadId": "thread-review",
            })
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("review-thread-read".to_string()),
                    method: "thread/read".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-review",
                        "includeTurns": false,
                    })),
                    trace: None,
                }))
                .expect("thread/read request should serialize")
                .into(),
            ))
            .await
            .expect("thread/read request should send");

        let JSONRPCMessage::Response(read_response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected thread/read response");
        };
        assert_eq!(
            read_response.id,
            RequestId::String("review-thread-read".to_string())
        );
        assert_eq!(
            read_response.result,
            serde_json::json!({
                "thread": {
                    "id": "thread-review",
                    "name": "Detached review thread",
                },
            })
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_filters_thread_list_responses_by_scope() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            "thread/list",
            serde_json::json!({
                "cursor": null,
                "limit": 10,
                "sortKey": null,
                "sortDirection": null,
                "modelProviders": null,
                "sourceKinds": null,
                "archived": null,
                "cwd": null,
                "searchTerm": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "id": "thread-visible",
                        "name": "Visible thread",
                    },
                    {
                        "id": "thread-hidden",
                        "name": "Hidden thread",
                    }
                ],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-list".to_string()),
                    method: "thread/list".to_string(),
                    params: Some(serde_json::json!({
                        "cursor": null,
                        "limit": 10,
                    })),
                    trace: None,
                }))
                .expect("thread list request should serialize")
                .into(),
            ))
            .await
            .expect("thread list request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected thread list response");
        };
        assert_eq!(response.id, RequestId::String("thread-list".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "data": [
                    {
                        "id": "thread-visible",
                        "name": "Visible thread",
                    }
                ],
                "nextCursor": null,
                "backwardsCursor": null,
            })
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_filters_thread_loaded_list_responses_by_scope() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            "thread/loaded/list",
            serde_json::json!({
                "cursor": null,
                "limit": 10,
            }),
            serde_json::json!({
                "data": ["thread-visible", "thread-hidden"],
                "nextCursor": null,
            }),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("thread-loaded-list".to_string()),
                    method: "thread/loaded/list".to_string(),
                    params: Some(serde_json::json!({
                        "cursor": null,
                        "limit": 10,
                    })),
                    trace: None,
                }))
                .expect("thread loaded list request should serialize")
                .into(),
            ))
            .await
            .expect("thread loaded list request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected thread loaded list response");
        };
        assert_eq!(
            response.id,
            RequestId::String("thread-loaded-list".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "data": ["thread-visible"],
                "nextCursor": null,
            })
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_rejects_hidden_downstream_server_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_hidden_server_request().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let hidden_request = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(hidden_request.is_err(), true);

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_item_delta_notifications_for_visible_threads() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_item_delta_notification().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected item delta notification");
        };
        assert_eq!(notification.method, "item/agentMessage/delta");
        assert_eq!(
            notification.params,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "turnId": "turn-visible",
                "itemId": "item-visible",
                "delta": "streamed text",
            }))
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_realtime_start_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_realtime_start().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("realtime-start".to_string()),
                    method: "thread/realtime/start".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "outputModality": "text",
                        "transport": {
                            "type": "websocket"
                        }
                    })),
                    trace: None,
                }))
                .expect("realtime start request should serialize")
                .into(),
            ))
            .await
            .expect("realtime start request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected realtime start response");
        };
        assert_eq!(response.id, RequestId::String("realtime-start".to_string()));
        assert_eq!(response.result, serde_json::json!({}));

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_realtime_append_text_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_realtime_append_text().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("realtime-append-text".to_string()),
                    method: "thread/realtime/appendText".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "text": "hello realtime",
                    })),
                    trace: None,
                }))
                .expect("realtime append text request should serialize")
                .into(),
            ))
            .await
            .expect("realtime append text request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected realtime append text response");
        };
        assert_eq!(
            response.id,
            RequestId::String("realtime-append-text".to_string())
        );
        assert_eq!(response.result, serde_json::json!({}));

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_realtime_append_audio_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_realtime_append_audio().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("realtime-append-audio".to_string()),
                    method: "thread/realtime/appendAudio".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "audio": {
                            "data": "AQID",
                            "sampleRate": 24000,
                            "numChannels": 1,
                            "samplesPerChannel": 3,
                            "itemId": "item-visible",
                        }
                    })),
                    trace: None,
                }))
                .expect("realtime append audio request should serialize")
                .into(),
            ))
            .await
            .expect("realtime append audio request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected realtime append audio response");
        };
        assert_eq!(
            response.id,
            RequestId::String("realtime-append-audio".to_string())
        );
        assert_eq!(response.result, serde_json::json!({}));

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_realtime_stop_requests() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_realtime_stop().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("realtime-stop".to_string()),
                    method: "thread/realtime/stop".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                    })),
                    trace: None,
                }))
                .expect("realtime stop request should serialize")
                .into(),
            ))
            .await
            .expect("realtime stop request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected realtime stop response");
        };
        assert_eq!(response.id, RequestId::String("realtime-stop".to_string()));
        assert_eq!(response.result, serde_json::json!({}));

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn websocket_upgrade_forwards_realtime_started_notifications_for_visible_threads() {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_realtime_started_notification().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                experimental_api: true,
                opt_out_notification_methods: None,
            }),
        )
        .await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            "thread/realtime/started",
            serde_json::json!({
                "threadId": "thread-visible",
                "sessionId": "realtime-session-1",
                "version": "v1",
            }),
        );

        server_task.abort();
        let _ = server_task.await;
    }

    #[test]
    fn enforce_request_scope_rejects_thread_resume_history_and_path_bypass() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();

        let history_error = super::enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-resume-history".to_string()),
                method: "thread/resume".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "history": [{}],
                })),
                trace: None,
            },
        )
        .expect_err("history-based resume should be rejected");
        assert_eq!(
            format!("{history_error:?}"),
            "InvalidRequest(\"gateway scope policy requires `thread/resume` to use `threadId` only\")"
        );

        let path_error = super::enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-resume-path".to_string()),
                method: "thread/resume".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            },
        )
        .expect_err("path-based resume should be rejected");
        assert_eq!(
            format!("{path_error:?}"),
            "InvalidRequest(\"gateway scope policy requires `thread/resume` to use `threadId` only\")"
        );
    }

    #[test]
    fn enforce_request_scope_rejects_thread_fork_path_bypass() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();

        let error = super::enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-fork-path".to_string()),
                method: "thread/fork".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            },
        )
        .expect_err("path-based fork should be rejected");
        assert_eq!(
            format!("{error:?}"),
            "InvalidRequest(\"gateway scope policy requires `thread/fork` to use `threadId` only\")"
        );
    }

    #[test]
    fn apply_response_scope_policy_registers_resume_and_fork_threads_and_filters_loaded_list() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );
        scope_registry.register_thread(
            "thread-hidden".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-b".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );

        let resume_result = super::apply_response_scope_policy(
            &scope_registry,
            &context,
            "thread/resume",
            serde_json::json!({
                "thread": {
                    "id": "thread-resumed",
                }
            }),
        )
        .expect("resume response should be accepted");
        assert_eq!(resume_result["thread"]["id"], "thread-resumed");
        assert_eq!(
            scope_registry.thread_visible_to(&context, "thread-resumed"),
            true
        );

        let fork_result = super::apply_response_scope_policy(
            &scope_registry,
            &context,
            "thread/fork",
            serde_json::json!({
                "thread": {
                    "id": "thread-forked",
                }
            }),
        )
        .expect("fork response should be accepted");
        assert_eq!(fork_result["thread"]["id"], "thread-forked");
        assert_eq!(
            scope_registry.thread_visible_to(&context, "thread-forked"),
            true
        );

        let filtered_result = super::apply_response_scope_policy(
            &scope_registry,
            &context,
            "thread/loaded/list",
            serde_json::json!({
                "data": ["thread-visible", "thread-hidden", "thread-forked"],
            }),
        )
        .expect("loaded list should be filtered");
        assert_eq!(
            filtered_result,
            serde_json::json!({
                "data": ["thread-visible", "thread-forked"],
            })
        );
    }

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

    async fn spawn_test_server(
        state: GatewayV2State,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        let app = Router::new().route(
            "/",
            any(websocket_upgrade_handler).with_state(state.clone()),
        );
        let server_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });
        (addr, server_task)
    }

    async fn read_websocket_message(
        websocket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
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
                        .expect("text frame should decode");
                }
                Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {
                    continue;
                }
                Message::Close(_) => panic!("unexpected close frame"),
            }
        }
    }

    async fn wait_for_close_frame(
        websocket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> Message {
        timeout(Duration::from_secs(2), async {
            loop {
                let frame = websocket
                    .next()
                    .await
                    .expect("websocket should yield frame")
                    .expect("frame should decode");
                if matches!(frame, Message::Close(_)) {
                    break frame;
                }
            }
        })
        .await
        .expect("close frame should arrive")
    }

    async fn test_initialize_response() -> InitializeResponse {
        let codex_home = tempdir().expect("tempdir");
        let config = Config::load_default_with_cli_overrides_for_codex_home(
            codex_home.path().to_path_buf(),
            Vec::new(),
        )
        .await
        .expect("config should load");
        gateway_initialize_response(&config)
    }

    async fn start_mock_remote_server_for_initialize() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            let frame = websocket
                .next()
                .await
                .expect("initialize frame should exist")
                .expect("initialize frame should decode");
            let Message::Text(text) = frame else {
                panic!("expected initialize text frame");
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("initialize should decode")
            else {
                panic!("expected initialize request");
            };
            assert_eq!(request.method, "initialize");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({}),
                    }))
                    .expect("initialize response should serialize")
                    .into(),
                ))
                .await
                .expect("initialize response should send");

            let frame = websocket
                .next()
                .await
                .expect("initialized frame should exist")
                .expect("initialized frame should decode");
            let Message::Text(text) = frame else {
                panic!("expected initialized text frame");
            };
            let JSONRPCMessage::Notification(notification) =
                serde_json::from_str(&text).expect("initialized should decode")
            else {
                panic!("expected initialized notification");
            };
            assert_eq!(notification.method, "initialized");

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
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_hidden_server_request() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            let frame = websocket
                .next()
                .await
                .expect("initialize frame should exist")
                .expect("initialize frame should decode");
            let Message::Text(text) = frame else {
                panic!("expected initialize text frame");
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("initialize should decode")
            else {
                panic!("expected initialize request");
            };
            assert_eq!(request.method, "initialize");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({}),
                    }))
                    .expect("initialize response should serialize")
                    .into(),
                ))
                .await
                .expect("initialize response should send");

            let frame = websocket
                .next()
                .await
                .expect("initialized frame should exist")
                .expect("initialized frame should decode");
            let Message::Text(text) = frame else {
                panic!("expected initialized text frame");
            };
            let JSONRPCMessage::Notification(notification) =
                serde_json::from_str(&text).expect("initialized should decode")
            else {
                panic!("expected initialized notification");
            };
            assert_eq!(notification.method, "initialized");

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                        id: RequestId::String("hidden-server-request".to_string()),
                        method: "item/commandExecution/requestApproval".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": "thread-hidden",
                            "turnId": "turn-hidden",
                            "itemId": "item-hidden",
                            "reason": "Need to run a hidden command",
                            "command": "pwd",
                        })),
                        trace: None,
                    }))
                    .expect("server request should serialize")
                    .into(),
                ))
                .await
                .expect("server request should send");

            let frame = websocket
                .next()
                .await
                .expect("server request response should exist")
                .expect("server request response should decode");
            let Message::Text(text) = frame else {
                panic!("expected server request response text frame");
            };
            let JSONRPCMessage::Error(error) =
                serde_json::from_str(&text).expect("server request response should decode")
            else {
                panic!("expected server request error");
            };
            assert_eq!(
                error.id,
                RequestId::String("hidden-server-request".to_string())
            );
            assert_eq!(error.error.code, super::INVALID_PARAMS_CODE);
            assert_eq!(error.error.message, "thread not found");

            tokio::time::sleep(Duration::from_millis(250)).await;
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_item_delta_notification() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                        method: "item/agentMessage/delta".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": "thread-visible",
                            "turnId": "turn-visible",
                            "itemId": "item-visible",
                            "delta": "streamed text",
                        })),
                    }))
                    .expect("notification should serialize")
                    .into(),
                ))
                .await
                .expect("notification should send");

            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_realtime_start() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            let frame = websocket
                .next()
                .await
                .expect("realtime request should exist")
                .expect("realtime request should decode");
            let Message::Text(text) = frame else {
                panic!("expected realtime request text frame");
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("realtime request should decode")
            else {
                panic!("expected realtime request");
            };
            assert_eq!(request.method, "thread/realtime/start");
            assert_eq!(
                request.params,
                Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "outputModality": "text",
                    "sessionId": null,
                    "transport": {
                        "type": "websocket"
                    },
                    "voice": null,
                }))
            );

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({}),
                    }))
                    .expect("realtime response should serialize")
                    .into(),
                ))
                .await
                .expect("realtime response should send");

            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_realtime_append_text() -> String {
        start_mock_remote_server_for_passthrough_request(
            "thread/realtime/appendText",
            serde_json::json!({
                "threadId": "thread-visible",
                "text": "hello realtime",
            }),
        )
        .await
    }

    async fn start_mock_remote_server_for_realtime_append_audio() -> String {
        start_mock_remote_server_for_passthrough_request(
            "thread/realtime/appendAudio",
            serde_json::json!({
                "threadId": "thread-visible",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-visible",
                }
            }),
        )
        .await
    }

    async fn start_mock_remote_server_for_realtime_stop() -> String {
        start_mock_remote_server_for_passthrough_request(
            "thread/realtime/stop",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
        )
        .await
    }

    async fn start_mock_remote_server_for_realtime_started_notification() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            send_remote_notification(
                &mut websocket,
                "thread/realtime/started",
                serde_json::json!({
                    "threadId": "thread-visible",
                    "sessionId": "realtime-session-1",
                    "version": "v1",
                }),
            )
            .await;

            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_passthrough_request(
        expected_method: &'static str,
        expected_params: serde_json::Value,
    ) -> String {
        start_mock_remote_server_for_passthrough_request_with_result(
            expected_method,
            expected_params,
            serde_json::json!({}),
        )
        .await
    }

    async fn start_mock_remote_server_for_passthrough_request_with_result(
        expected_method: &'static str,
        expected_params: serde_json::Value,
        response_result: serde_json::Value,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            let frame = websocket
                .next()
                .await
                .expect("request should exist")
                .expect("request should decode");
            let Message::Text(text) = frame else {
                panic!("expected request text frame");
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("request should decode")
            else {
                panic!("expected request");
            };
            assert_eq!(request.method, expected_method);
            assert_eq!(request.params, Some(expected_params));

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: response_result,
                    }))
                    .expect("response should serialize")
                    .into(),
                ))
                .await
                .expect("response should send");

            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        format!("ws://{addr}")
    }

    async fn start_mock_remote_server_for_review_start_then_thread_read() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            let review_start = websocket
                .next()
                .await
                .expect("review/start request should exist")
                .expect("review/start request should decode");
            let Message::Text(review_start_text) = review_start else {
                panic!("expected review/start text frame");
            };
            let JSONRPCMessage::Request(review_start_request) =
                serde_json::from_str(&review_start_text).expect("review/start should decode")
            else {
                panic!("expected review/start request");
            };
            assert_eq!(review_start_request.method, "review/start");
            assert_eq!(
                review_start_request.params,
                Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "target": {
                        "type": "custom",
                        "instructions": "Review the current change",
                    },
                    "delivery": "detached",
                }))
            );
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: review_start_request.id,
                        result: serde_json::json!({
                            "turn": {
                                "id": "turn-review",
                                "items": [],
                                "status": "pending",
                                "error": null,
                                "startedAt": 1,
                                "completedAt": null,
                                "durationMs": null,
                            },
                            "reviewThreadId": "thread-review",
                        }),
                    }))
                    .expect("review/start response should serialize")
                    .into(),
                ))
                .await
                .expect("review/start response should send");

            let thread_read = websocket
                .next()
                .await
                .expect("thread/read request should exist")
                .expect("thread/read request should decode");
            let Message::Text(thread_read_text) = thread_read else {
                panic!("expected thread/read text frame");
            };
            let JSONRPCMessage::Request(thread_read_request) =
                serde_json::from_str(&thread_read_text).expect("thread/read should decode")
            else {
                panic!("expected thread/read request");
            };
            assert_eq!(thread_read_request.method, "thread/read");
            assert_eq!(
                thread_read_request.params,
                Some(serde_json::json!({
                    "threadId": "thread-review",
                    "includeTurns": false,
                }))
            );
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: thread_read_request.id,
                        result: serde_json::json!({
                            "thread": {
                                "id": "thread-review",
                                "name": "Detached review thread",
                            },
                        }),
                    }))
                    .expect("thread/read response should serialize")
                    .into(),
                ))
                .await
                .expect("thread/read response should send");

            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        format!("ws://{addr}")
    }

    async fn spawn_remote_gateway_v2_test_server(
        websocket_url: String,
        scope_registry: Arc<GatewayScopeRegistry>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let initialize_response = test_initialize_response().await;
        spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    websocket_url,
                    auth_token: None,
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await
    }

    async fn start_mock_remote_server_that_disconnects_after_initialize() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            websocket
                .close(None)
                .await
                .expect("close frame should send");
        });
        format!("ws://{addr}")
    }

    async fn send_remote_notification(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        method: &str,
        params: serde_json::Value,
    ) {
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                    method: method.to_string(),
                    params: Some(params),
                }))
                .expect("notification should serialize")
                .into(),
            ))
            .await
            .expect("notification should send");
    }

    fn assert_jsonrpc_notification(
        message: JSONRPCMessage,
        expected_method: &str,
        expected_params: serde_json::Value,
    ) {
        let JSONRPCMessage::Notification(notification) = message else {
            panic!("expected notification");
        };
        assert_eq!(notification.method, expected_method);
        assert_eq!(notification.params, Some(expected_params));
    }

    async fn expect_remote_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
        let frame = websocket
            .next()
            .await
            .expect("initialize frame should exist")
            .expect("initialize frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialize text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("initialize should decode")
        else {
            panic!("expected initialize request");
        };
        assert_eq!(request.method, "initialize");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("initialize response should serialize")
                .into(),
            ))
            .await
            .expect("initialize response should send");

        let frame = websocket
            .next()
            .await
            .expect("initialized frame should exist")
            .expect("initialized frame should decode");
        let Message::Text(text) = frame else {
            panic!("expected initialized text frame");
        };
        let JSONRPCMessage::Notification(notification) =
            serde_json::from_str(&text).expect("initialized should decode")
        else {
            panic!("expected initialized notification");
        };
        assert_eq!(notification.method, "initialized");
    }

    async fn send_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) {
        send_initialize_with_capabilities(websocket, None).await;
    }

    async fn send_initialize_with_capabilities(
        websocket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        capabilities: Option<InitializeCapabilities>,
    ) {
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("initialize".to_string()),
                    method: "initialize".to_string(),
                    params: Some(
                        serde_json::to_value(InitializeParams {
                            client_info: ClientInfo {
                                name: "codex-tui".to_string(),
                                title: None,
                                version: "0.0.0-test".to_string(),
                            },
                            capabilities,
                        })
                        .expect("initialize params should serialize"),
                    ),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("initialize request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(websocket).await else {
            panic!("expected initialize response");
        };
        assert_eq!(response.id, RequestId::String("initialize".to_string()));
    }
}
