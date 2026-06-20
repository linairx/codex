use crate::admission::GatewayAdmissionController;
use crate::auth::GatewayAuthError;
use crate::auth::is_authorized;
use crate::northbound::v2::GatewayV2State;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::northbound::v2::INITIALIZE_TIMEOUT_CLOSE_REASON;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::INVALID_REQUEST_CODE;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection_runtime::GatewayV2ConnectionRunResult;
use crate::northbound::v2_handshake::recv_initialize_request;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::downstream_protocol_violation_reason;
use crate::northbound::v2_wire::log_downstream_connect_protocol_violation;
use crate::northbound::v2_wire::observe_client_response_send_failure;
use crate::northbound::v2_wire::observe_v2_connection;
use crate::northbound::v2_wire::observe_v2_request;
use crate::northbound::v2_wire::request_params;
use crate::northbound::v2_wire_send::send_jsonrpc;
use crate::northbound::v2_wire_send::send_jsonrpc_error;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::ORIGIN;
use axum::response::IntoResponse;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCResponse;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

pub(crate) async fn run_websocket_connection(
    mut socket: WebSocket,
    session_factory: Arc<GatewayV2SessionFactory>,
    admission: GatewayAdmissionController,
    observability: GatewayObservability,
    scope_registry: Arc<GatewayScopeRegistry>,
    context: GatewayRequestContext,
    timeouts: GatewayV2Timeouts,
) -> io::Result<()> {
    let connection_started_at = Instant::now();
    let initialize_started_at = Instant::now();
    let connection_id = observability
        .v2_connection_health()
        .mark_connection_started();
    let run_result = match async {
        let initialize_request =
            match recv_initialize_request(&mut socket, timeouts, &observability, &context).await {
                Ok(request) => request,
                Err(err) if err.kind() == ErrorKind::TimedOut => {
                    send_observed_close_frame(
                        &mut socket,
                        &observability,
                        &context,
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
                    return Ok(GatewayV2ConnectionRunResult {
                        outcome: "initialize_timed_out",
                        detail: Some(INITIALIZE_TIMEOUT_CLOSE_REASON.to_string()),
                        pending_client_request_count: 0,
                        pending_client_request_worker_counts: Vec::new(),
                        pending_client_request_method_counts: Vec::new(),
                        pending_server_request_count: 0,
                        answered_but_unresolved_server_request_count: 0,
                        server_request_backlog_worker_counts: Vec::new(),
                        server_request_backlog_method_counts: Vec::new(),
                        result: Ok(()),
                    });
                }
                Err(err) if err.kind() == ErrorKind::InvalidData => {
                    return Ok(GatewayV2ConnectionRunResult {
                        outcome: "invalid_client_payload",
                        detail: Some(err.to_string()),
                        pending_client_request_count: 0,
                        pending_client_request_worker_counts: Vec::new(),
                        pending_client_request_method_counts: Vec::new(),
                        pending_server_request_count: 0,
                        answered_but_unresolved_server_request_count: 0,
                        server_request_backlog_worker_counts: Vec::new(),
                        server_request_backlog_method_counts: Vec::new(),
                        result: Ok(()),
                    });
                }
                Err(err) => return Err(err),
            };
        let initialize_request_id = initialize_request.id.clone();
        let initialize_params = match request_params::<InitializeParams>(&initialize_request) {
            Ok(params) => params,
            Err(err) => {
                let detail = err.to_string();
                send_jsonrpc_error(
                    &mut socket,
                    initialize_request_id.clone(),
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_CODE,
                        message: detail.clone(),
                        data: None,
                    },
                    timeouts.client_send,
                )
                .await
                .inspect_err(|err| {
                    let outcome = classify_v2_connection_error(err);
                    observe_client_response_send_failure(
                        &observability,
                        &context,
                        &initialize_request_id,
                        "initialize",
                        outcome,
                        err,
                    );
                })?;
                observe_v2_request(
                    &observability,
                    &context,
                    "initialize",
                    "invalid_request",
                    initialize_started_at.elapsed(),
                );
                return Ok(GatewayV2ConnectionRunResult {
                    outcome: "initialize_invalid_request",
                    detail: Some(detail),
                    pending_client_request_count: 0,
                    pending_client_request_worker_counts: Vec::new(),
                    pending_client_request_method_counts: Vec::new(),
                    pending_server_request_count: 0,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_worker_counts: Vec::new(),
                    server_request_backlog_method_counts: Vec::new(),
                    result: Ok(()),
                });
            }
        };
        let downstream = match GatewayV2DownstreamRouter::connect_with_timeouts(
            &session_factory,
            &initialize_params,
            &context,
            &timeouts,
        )
        .await
        {
            Ok(downstream) => downstream,
            Err(err) => {
                let detail = err.to_string();
                let downstream_protocol_violation_reason =
                    downstream_protocol_violation_reason(&detail);
                let request_outcome = if downstream_protocol_violation_reason.is_some() {
                    "downstream_protocol_violation"
                } else {
                    "downstream_connect_error"
                };
                if let Some(reason) = downstream_protocol_violation_reason {
                    log_downstream_connect_protocol_violation(&context, reason, &detail);
                    observability.record_v2_protocol_violation("downstream", reason);
                }
                observe_v2_request(
                    &observability,
                    &context,
                    "initialize",
                    request_outcome,
                    initialize_started_at.elapsed(),
                );
                send_jsonrpc_error(
                    &mut socket,
                    initialize_request_id.clone(),
                    JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("gateway failed to connect downstream app-server: {err}"),
                        data: None,
                    },
                    timeouts.client_send,
                )
                .await
                .inspect_err(|err| {
                    let outcome = classify_v2_connection_error(err);
                    observe_client_response_send_failure(
                        &observability,
                        &context,
                        &initialize_request_id,
                        "initialize",
                        outcome,
                        err,
                    );
                })?;
                return Ok(GatewayV2ConnectionRunResult {
                    outcome: request_outcome,
                    detail: Some(detail),
                    pending_client_request_count: 0,
                    pending_client_request_worker_counts: Vec::new(),
                    pending_client_request_method_counts: Vec::new(),
                    pending_server_request_count: 0,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_worker_counts: Vec::new(),
                    server_request_backlog_method_counts: Vec::new(),
                    result: Ok(()),
                });
            }
        };
        send_jsonrpc(
            &mut socket,
            JSONRPCMessage::Response(JSONRPCResponse {
                id: initialize_request_id.clone(),
                result: serde_json::to_value(session_factory.initialize_response())
                    .map_err(io::Error::other)?,
            }),
            timeouts.client_send,
        )
        .await
        .inspect_err(|err| {
            let outcome = classify_v2_connection_error(err);
            observe_client_response_send_failure(
                &observability,
                &context,
                &initialize_request_id,
                "initialize",
                outcome,
                err,
            );
        })?;
        observe_v2_request(
            &observability,
            &context,
            "initialize",
            "ok",
            initialize_started_at.elapsed(),
        );

        let connection = GatewayV2ConnectionContext {
            admission: &admission,
            observability: &observability,
            scope_registry: &scope_registry,
            request_context: &context,
            client_send_timeout: timeouts.client_send,
            max_pending_server_requests: timeouts.max_pending_server_requests,
            max_pending_client_requests: timeouts.max_pending_client_requests,
            opt_out_notification_methods: initialize_params
                .capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.opt_out_notification_methods.clone())
                .unwrap_or_default()
                .into_iter()
                .collect(),
        };
        let run_result = crate::northbound::v2_connection_post_initialize::run_post_initialize_websocket_connection(
            socket,
            downstream,
            session_factory,
            &connection,
            initialize_params,
            connection_id,
            timeouts,
        )
        .await?;
        Ok(run_result)
    }
    .await
    {
        Ok(run_result) => run_result,
        Err(err) => GatewayV2ConnectionRunResult {
            outcome: classify_v2_connection_error(&err),
            detail: Some(err.to_string()),
            pending_client_request_count: 0,
            pending_client_request_worker_counts: Vec::new(),
            pending_client_request_method_counts: Vec::new(),
            pending_server_request_count: 0,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_worker_counts: Vec::new(),
            server_request_backlog_method_counts: Vec::new(),
            result: Err(err),
        },
    };
    observe_v2_connection(
        &observability,
        connection_id,
        &context,
        run_result.outcome,
        run_result.detail.as_deref(),
        GatewayV2ConnectionPendingCounts {
            pending_client_request_count: run_result.pending_client_request_count,
            pending_client_request_worker_counts: run_result
                .pending_client_request_worker_counts
                .clone(),
            pending_client_request_method_counts: run_result
                .pending_client_request_method_counts
                .clone(),
            pending_server_request_count: run_result.pending_server_request_count,
            answered_but_unresolved_server_request_count: run_result
                .answered_but_unresolved_server_request_count,
            server_request_backlog_worker_counts: run_result
                .server_request_backlog_worker_counts
                .clone(),
            server_request_backlog_method_counts: run_result
                .server_request_backlog_method_counts
                .clone(),
        },
        connection_started_at.elapsed(),
    );
    run_result.result
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
