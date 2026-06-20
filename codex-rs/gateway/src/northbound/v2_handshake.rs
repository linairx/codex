use crate::northbound::v2::GatewayV2Timeouts;
use crate::northbound::v2::INITIALIZE_TIMEOUT_CLOSE_REASON;
use crate::northbound::v2::INVALID_REQUEST_CODE;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::observe_client_response_send_failure;
use crate::northbound::v2_wire::observe_v2_request;
use crate::northbound::v2_wire::parse_client_jsonrpc_binary;
use crate::northbound::v2_wire::parse_client_jsonrpc_text;
use crate::northbound::v2_wire::protocol_violation_reason_from_invalid_payload;
use crate::northbound::v2_wire_send::send_jsonrpc_error;
use crate::northbound::v2_wire_send::send_observed_invalid_payload_close;
use crate::northbound::v2_wire_send::send_websocket_message;
use crate::scope::GatewayRequestContext;
use axum::extract::ws::Message as WebSocketMessage;
use axum::extract::ws::WebSocket;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCRequest;
use std::io;
use std::io::ErrorKind;
use std::time::Duration;
use std::time::Instant;

pub(crate) async fn recv_initialize_request(
    socket: &mut WebSocket,
    timeouts: GatewayV2Timeouts,
    observability: &crate::observability::GatewayObservability,
    context: &GatewayRequestContext,
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
                            observability.record_v2_protocol_violation(
                                "pre_initialize",
                                protocol_violation_reason_from_invalid_payload(&err),
                            );
                            send_observed_invalid_payload_close(
                                socket,
                                observability,
                                context,
                                &err,
                                timeouts.client_send,
                            )
                            .await?;
                            return Err(err);
                        }
                    };
                    if let Some(request) = handle_pre_initialize_message(
                        socket,
                        message,
                        timeouts.client_send,
                        observability,
                        context,
                    )
                    .await?
                    {
                        return Ok(request);
                    }
                }
                Ok(WebSocketMessage::Binary(bytes)) => {
                    let message = match parse_client_jsonrpc_binary(&bytes) {
                        Ok(message) => message,
                        Err(err) => {
                            observability.record_v2_protocol_violation(
                                "pre_initialize",
                                protocol_violation_reason_from_invalid_payload(&err),
                            );
                            send_observed_invalid_payload_close(
                                socket,
                                observability,
                                context,
                                &err,
                                timeouts.client_send,
                            )
                            .await?;
                            return Err(err);
                        }
                    };
                    if let Some(request) = handle_pre_initialize_message(
                        socket,
                        message,
                        timeouts.client_send,
                        observability,
                        context,
                    )
                    .await?
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

async fn handle_pre_initialize_message(
    socket: &mut WebSocket,
    message: JSONRPCMessage,
    client_send_timeout: Duration,
    observability: &crate::observability::GatewayObservability,
    context: &GatewayRequestContext,
) -> io::Result<Option<JSONRPCRequest>> {
    match message {
        JSONRPCMessage::Request(request) if request.method == "initialize" => Ok(Some(request)),
        JSONRPCMessage::Request(request) => {
            let started_at = Instant::now();
            let method = request.method.clone();
            observability.record_v2_protocol_violation("pre_initialize", "initialize_order");
            observe_v2_request(
                observability,
                context,
                &method,
                "invalid_request",
                started_at.elapsed(),
            );
            send_jsonrpc_error(
                socket,
                request.id.clone(),
                JSONRPCErrorError {
                    code: INVALID_REQUEST_CODE,
                    message: "initialize must be the first request".to_string(),
                    data: None,
                },
                client_send_timeout,
            )
            .await
            .inspect_err(|err| {
                let outcome = classify_v2_connection_error(err);
                observe_client_response_send_failure(
                    observability,
                    context,
                    &request.id,
                    &method,
                    outcome,
                    err,
                );
            })?;
            Ok(None)
        }
        JSONRPCMessage::Notification(_)
        | JSONRPCMessage::Response(_)
        | JSONRPCMessage::Error(_) => Ok(None),
    }
}
