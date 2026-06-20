use crate::northbound::v2::DOWNSTREAM_SESSION_ENDED_CLOSE_REASON;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::northbound::v2_client_response::handle_pending_client_response;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientResponses;
use crate::northbound::v2_connection_event::handle_app_server_event;
use crate::northbound::v2_connection_runtime::GatewayV2ConnectionRunResult;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::parse_client_jsonrpc_binary;
use crate::northbound::v2_wire::parse_client_jsonrpc_text;
use crate::northbound::v2_wire::protocol_violation_reason_from_invalid_payload;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use crate::northbound::v2_wire_send::send_observed_invalid_payload_close;
use crate::northbound::v2_wire_send::send_websocket_message;
use crate::v2::GatewayV2SessionFactory;
use axum::extract::ws::Message as WebSocketMessage;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_protocol::InitializeParams;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;

pub(crate) async fn run_post_initialize_websocket_connection(
    mut socket: WebSocket,
    mut downstream: GatewayV2DownstreamRouter,
    session_factory: Arc<GatewayV2SessionFactory>,
    connection: &GatewayV2ConnectionContext<'_>,
    _initialize_params: InitializeParams,
    connection_id: u64,
    timeouts: GatewayV2Timeouts,
) -> io::Result<GatewayV2ConnectionRunResult> {
    let mut event_state = GatewayV2EventState {
        pending_server_requests: HashMap::new(),
        resolved_server_requests: HashMap::new(),
        skills_changed_pending_refresh: false,
        forwarded_connection_notifications: HashMap::new(),
    };
    let mut reject_pending_server_requests_on_exit = false;
    let mut connection_detail = None;
    let (pending_client_response_tx, mut pending_client_response_rx) =
        tokio::sync::mpsc::channel::<crate::northbound::v2_connection::PendingClientResponse>(
            timeouts.max_pending_client_requests.max(1),
        );
    let mut pending_client_responses = PendingClientResponses {
        tx: pending_client_response_tx,
        tasks: Vec::new(),
        count: 0,
        active: HashMap::new(),
    };
    let mut reconnect_tick = tokio::time::interval_at(
        tokio::time::Instant::now() + timeouts.reconnect_retry_backoff,
        timeouts.reconnect_retry_backoff,
    );
    let (loop_result, connection_outcome): (io::Result<()>, &'static str) = loop {
        tokio::select! {
            frame = socket.recv() => {
                let Some(frame) = frame else {
                    break (Ok(()), "client_disconnected");
                };
                match frame {
                    Ok(WebSocketMessage::Text(text)) => {
                        let message = match parse_client_jsonrpc_text(&text) {
                            Ok(message) => message,
                            Err(err) => {
                                connection_detail = Some(err.to_string());
                                connection.observability.record_v2_protocol_violation(
                                    "post_initialize",
                                    protocol_violation_reason_from_invalid_payload(&err),
                                );
                                send_observed_invalid_payload_close(
                                    &mut socket,
                                    connection.observability,
                                    connection.request_context,
                                    &err,
                                    timeouts.client_send,
                                )
                                .await?;
                                reject_pending_server_requests_on_exit = true;
                                break (Ok(()), "invalid_client_payload");
                            }
                        };
                        if let Err(err) = crate::northbound::v2_client_message::handle_client_message(
                            &mut socket,
                            &mut downstream,
                            connection,
                            &mut event_state,
                            &mut pending_client_responses,
                            message,
                        )
                        .await {
                            let outcome = classify_v2_connection_error(&err);
                            break (Err(err), outcome);
                        }
                        crate::northbound::v2_connection_runtime::update_active_v2_connection_pending_counts(
                            connection.observability,
                            connection_id,
                            &event_state,
                            &pending_client_responses.active,
                        );
                    }
                    Ok(WebSocketMessage::Binary(bytes)) => {
                        let message = match parse_client_jsonrpc_binary(&bytes) {
                            Ok(message) => message,
                            Err(err) => {
                                connection_detail = Some(err.to_string());
                                connection.observability.record_v2_protocol_violation(
                                    "post_initialize",
                                    protocol_violation_reason_from_invalid_payload(&err),
                                );
                                send_observed_invalid_payload_close(
                                    &mut socket,
                                    connection.observability,
                                    connection.request_context,
                                    &err,
                                    timeouts.client_send,
                                )
                                .await?;
                                reject_pending_server_requests_on_exit = true;
                                break (Ok(()), "invalid_client_payload");
                            }
                        };
                        if let Err(err) = crate::northbound::v2_client_message::handle_client_message(
                            &mut socket,
                            &mut downstream,
                            connection,
                            &mut event_state,
                            &mut pending_client_responses,
                            message,
                        )
                        .await {
                            let outcome = classify_v2_connection_error(&err);
                            break (Err(err), outcome);
                        }
                        crate::northbound::v2_connection_runtime::update_active_v2_connection_pending_counts(
                            connection.observability,
                            connection_id,
                            &event_state,
                            &pending_client_responses.active,
                        );
                    }
                    Ok(WebSocketMessage::Close(_)) => {
                        reject_pending_server_requests_on_exit = true;
                        break (Ok(()), "client_closed");
                    }
                    Ok(WebSocketMessage::Ping(payload)) => {
                        send_websocket_message(
                            &mut socket,
                            WebSocketMessage::Pong(payload),
                            timeouts.client_send,
                        )
                        .await?;
                    }
                    Ok(WebSocketMessage::Pong(_)) => {}
                    Err(err) => {
                        let err = io::Error::other(format!("gateway websocket receive failed: {err}"));
                        let outcome = classify_v2_connection_error(&err);
                        break (Err(err), outcome);
                    }
                }
            }
            event = downstream.next_event() => {
                let Some(event) = event else {
                    send_observed_close_frame(
                        &mut socket,
                        connection.observability,
                        connection.request_context,
                        close_code::ERROR,
                        DOWNSTREAM_SESSION_ENDED_CLOSE_REASON,
                        timeouts.client_send,
                    )
                    .await?;
                    break (Ok(()), "downstream_session_ended");
                };
                let should_close = handle_app_server_event(
                    &mut socket,
                    &mut downstream,
                    &session_factory,
                    connection,
                    &mut event_state,
                    &pending_client_responses.active,
                    event,
                )
                .await;
                let should_close = match should_close {
                    Ok(should_close) => should_close,
                    Err(err) => {
                        let outcome = classify_v2_connection_error(&err);
                        break (Err(err), outcome);
                    }
                };
                if let Some(close) = should_close {
                    reject_pending_server_requests_on_exit = close.reject_pending_server_requests;
                    crate::northbound::v2_connection_runtime::update_active_v2_connection_pending_counts(
                        connection.observability,
                        connection_id,
                        &event_state,
                        &pending_client_responses.active,
                    );
                    break (Ok(()), close.outcome);
                }
                crate::northbound::v2_connection_runtime::update_active_v2_connection_pending_counts(
                    connection.observability,
                    connection_id,
                    &event_state,
                    &pending_client_responses.active,
                );
            }
            _ = reconnect_tick.tick() => {
                if downstream.reconnect_state.is_some() {
                    downstream
                        .reconnect_missing_workers(connection.observability)
                        .await;
                }
            }
            pending_response = pending_client_response_rx.recv() => {
                let Some(pending_response) = pending_response else {
                    continue;
                };
                if let Err(err) = handle_pending_client_response(
                    &mut socket,
                    connection,
                    connection_id,
                    &event_state,
                    &mut pending_client_responses,
                    pending_response,
                    timeouts.client_send,
                )
                .await
                {
                    let outcome = classify_v2_connection_error(&err);
                    break (Err(err), outcome);
                }
            }
        }
    };

    crate::northbound::v2_connection_finalize::finalize_websocket_connection(
        crate::northbound::v2_connection_finalize::GatewayV2ConnectionFinalization {
            downstream,
            connection,
            event_state,
            pending_client_responses,
            pending_client_response_rx: &mut pending_client_response_rx,
            connection_outcome,
            connection_detail,
            reject_pending_server_requests_on_exit,
            loop_result,
        },
    )
    .await
}
