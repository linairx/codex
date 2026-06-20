use crate::northbound::v2::DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::INVALID_REQUEST_CODE;
use crate::northbound::v2::UNEXPECTED_CLIENT_SERVER_REQUEST_RESPONSE_CLOSE_REASON;
use crate::northbound::v2_connection::ClientServerRequestAnswer;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientRequestRoute;
use crate::northbound::v2_connection::PendingClientResponse;
use crate::northbound::v2_connection::PendingClientResponses;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_connection_lifecycle::log_duplicate_pending_client_request;
use crate::northbound::v2_connection_lifecycle::log_unexpected_client_server_request_response;
use crate::northbound::v2_connection_runtime::deliver_client_server_request_answer;
use crate::northbound::v2_connection_runtime::log_fail_closed_multi_worker_request;
use crate::northbound::v2_limits::pending_client_request_limit_error;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_request_routing::fanout_connection_notification;
use crate::northbound::v2_routing::worker_for_notification;
use crate::northbound::v2_routing::worker_for_request;
use crate::northbound::v2_scope::enforce_request_scope;
use crate::northbound::v2_wire::gateway_error_outcome;
use crate::northbound::v2_wire::gateway_error_to_jsonrpc_error;
use crate::northbound::v2_wire::jsonrpc_error_outcome_code;
use crate::northbound::v2_wire::jsonrpc_notification_to_client_notification;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::observe_v2_request;
use crate::northbound::v2_wire_send::send_client_jsonrpc;
use crate::northbound::v2_wire_send::send_client_jsonrpc_error;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use crate::northbound::v2_wire_send::send_observed_invalid_payload_close;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCResponse;
use std::io;
use std::io::ErrorKind;
use std::time::Instant;

pub(crate) async fn handle_client_message(
    socket: &mut WebSocket,
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    event_state: &mut GatewayV2EventState,
    pending_client_responses: &mut PendingClientResponses,
    message: JSONRPCMessage,
) -> io::Result<()> {
    match message {
        JSONRPCMessage::Request(request) => {
            let started_at = Instant::now();
            let method = request.method.clone();
            if request.method == "initialize" {
                connection
                    .observability
                    .record_v2_protocol_violation("post_initialize", "repeated_initialize");
                send_client_jsonrpc_error(
                    socket,
                    connection,
                    &request.id,
                    &method,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_CODE,
                        message: "connection is already initialized".to_string(),
                        data: None,
                    },
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
                send_client_jsonrpc_error(
                    socket,
                    connection,
                    &request.id,
                    &method,
                    gateway_error_to_jsonrpc_error(err),
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
                send_client_jsonrpc_error(
                    socket,
                    connection,
                    &request.id,
                    &method,
                    gateway_error_to_jsonrpc_error(err),
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
            let method = request.method.clone();
            if pending_client_responses.active.contains_key(&request_id) {
                log_duplicate_pending_client_request(
                    connection.request_context,
                    &request_id,
                    &method,
                    &pending_client_responses.active,
                );
                connection
                    .observability
                    .record_v2_protocol_violation("post_initialize", "duplicate_request_id");
                observe_v2_request(
                    connection.observability,
                    connection.request_context,
                    &method,
                    "protocol_violation",
                    started_at.elapsed(),
                );
                send_observed_close_frame(
                    socket,
                    connection.observability,
                    connection.request_context,
                    close_code::PROTOCOL,
                    DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON,
                    connection.client_send_timeout,
                )
                .await?;
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON,
                ));
            }
            if method == "command/exec" {
                if let Some(error) = pending_client_request_limit_error(
                    pending_client_responses.count,
                    connection.max_pending_client_requests,
                ) {
                    crate::northbound::v2_connection_lifecycle::log_rejected_saturated_client_request(
                        connection.request_context,
                        &request_id,
                        &method,
                        pending_client_responses.count,
                        connection.max_pending_client_requests,
                    );
                    connection
                        .observability
                        .record_v2_client_request_rejection(&method, "pending_limit");
                    send_client_jsonrpc_error(socket, connection, &request_id, &method, error)
                        .await?;
                    observe_v2_request(
                        connection.observability,
                        connection.request_context,
                        &method,
                        "rate_limited",
                        started_at.elapsed(),
                    );
                    return Ok(());
                }
                if downstream.reconnect_state.is_some() {
                    downstream
                        .reconnect_missing_workers(connection.observability)
                        .await;
                }
                let worker = match worker_for_request(
                    downstream,
                    connection.scope_registry,
                    connection.request_context,
                    &request,
                ) {
                    Ok(worker) => worker.clone(),
                    Err(err) => {
                        log_fail_closed_multi_worker_request(
                            downstream,
                            connection,
                            method.as_str(),
                            &err,
                        );
                        observe_v2_request(
                            connection.observability,
                            connection.request_context,
                            &method,
                            "internal_error",
                            started_at.elapsed(),
                        );
                        return Err(err);
                    }
                };
                let client_request = jsonrpc_request_to_client_request(request)?;
                let request_context = connection.request_context.clone();
                let pending_client_response_tx = pending_client_responses.tx.clone();
                pending_client_responses.count += 1;
                pending_client_responses.active.insert(
                    request_id.clone(),
                    PendingClientRequestRoute {
                        method: method.clone(),
                        request_context: request_context.clone(),
                        worker_id: worker.worker_id,
                        worker_websocket_url: worker
                            .worker_websocket_url
                            .clone()
                            .unwrap_or_else(|| "<embedded>".to_string()),
                        started_at,
                    },
                );
                pending_client_responses
                    .tasks
                    .push(tokio::spawn(async move {
                        let result = worker.request_handle.request(client_request).await;
                        let _ = pending_client_response_tx
                            .send(PendingClientResponse {
                                request_id,
                                method,
                                request_context,
                                started_at,
                                result,
                            })
                            .await;
                    }));
                return Ok(());
            }
            match handle_client_request(downstream, connection, request).await {
                Ok(Ok(result)) => {
                    send_client_jsonrpc(
                        socket,
                        connection,
                        &request_id,
                        &method,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request_id.clone(),
                            result,
                        }),
                    )
                    .await?;
                    if downstream.multi_worker_topology() && method == "skills/list" {
                        event_state.skills_changed_pending_refresh = false;
                    }
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
                    send_client_jsonrpc_error(socket, connection, &request_id, &method, error)
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
                    send_client_jsonrpc_error(
                        socket,
                        connection,
                        &request_id,
                        &method,
                        JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("gateway upstream request failed: {err}"),
                            data: None,
                        },
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
            let is_initialized = notification.method == "initialized";
            if is_initialized {
                downstream.mark_initialized();
            }
            if is_initialized && downstream.multi_worker_topology() {
                fanout_connection_notification(downstream, ClientNotification::Initialized).await?;
                return Ok(());
            }
            let worker =
                worker_for_notification(downstream, connection.scope_registry, &notification)?;
            let client_notification = jsonrpc_notification_to_client_notification(notification)?;
            worker.request_handle.notify(client_notification).await?;
        }
        JSONRPCMessage::Response(response) => {
            let gateway_request_id = response.id.clone();
            let Some(route) = event_state.pending_server_requests.remove(&response.id) else {
                log_unexpected_client_server_request_response(
                    connection.request_context,
                    "response",
                    &response.id,
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                );
                connection
                    .observability
                    .record_v2_server_request_lifecycle_event(
                        "unexpected_client_server_request_response",
                        "response",
                    );
                let err = io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "{UNEXPECTED_CLIENT_SERVER_REQUEST_RESPONSE_CLOSE_REASON}: {:?}",
                        response.id
                    ),
                );
                send_observed_invalid_payload_close(
                    socket,
                    connection.observability,
                    connection.request_context,
                    &err,
                    connection.client_send_timeout,
                )
                .await?;
                return Err(err);
            };
            event_state.resolved_server_requests.insert(
                DownstreamServerRequestKey {
                    worker_id: route.worker_id,
                    request_id: route.downstream_request_id.clone(),
                },
                ResolvedServerRequestRoute {
                    gateway_request_id: gateway_request_id.clone(),
                    worker_websocket_url: route.worker_websocket_url.clone(),
                    method: route.method.clone(),
                    thread_id: route.thread_id.clone(),
                },
            );
            deliver_client_server_request_answer(
                downstream,
                connection,
                gateway_request_id,
                route,
                ClientServerRequestAnswer::Response(response.result),
            )
            .await?;
        }
        JSONRPCMessage::Error(error) => {
            let gateway_request_id = error.id.clone();
            let Some(route) = event_state.pending_server_requests.remove(&error.id) else {
                log_unexpected_client_server_request_response(
                    connection.request_context,
                    "error",
                    &error.id,
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                );
                connection
                    .observability
                    .record_v2_server_request_lifecycle_event(
                        "unexpected_client_server_request_response",
                        "error",
                    );
                let err = io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "{UNEXPECTED_CLIENT_SERVER_REQUEST_RESPONSE_CLOSE_REASON}: {:?}",
                        error.id
                    ),
                );
                send_observed_invalid_payload_close(
                    socket,
                    connection.observability,
                    connection.request_context,
                    &err,
                    connection.client_send_timeout,
                )
                .await?;
                return Err(err);
            };
            event_state.resolved_server_requests.insert(
                DownstreamServerRequestKey {
                    worker_id: route.worker_id,
                    request_id: route.downstream_request_id.clone(),
                },
                ResolvedServerRequestRoute {
                    gateway_request_id: gateway_request_id.clone(),
                    worker_websocket_url: route.worker_websocket_url.clone(),
                    method: route.method.clone(),
                    thread_id: route.thread_id.clone(),
                },
            );
            deliver_client_server_request_answer(
                downstream,
                connection,
                gateway_request_id,
                route,
                ClientServerRequestAnswer::Error(error.error),
            )
            .await?;
        }
    }

    Ok(())
}
