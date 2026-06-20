use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientResponse;
use crate::northbound::v2_connection::PendingClientResponses;
use crate::northbound::v2_connection_runtime::update_active_v2_connection_pending_counts;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::jsonrpc_error_outcome_code;
use crate::northbound::v2_wire::observe_client_response_send_failure;
use crate::northbound::v2_wire::observe_v2_request;
use crate::northbound::v2_wire_send::send_jsonrpc;
use crate::northbound::v2_wire_send::send_jsonrpc_error;
use axum::extract::ws::WebSocket;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCResponse;
use std::io;

pub(crate) async fn handle_pending_client_response(
    socket: &mut WebSocket,
    connection: &GatewayV2ConnectionContext<'_>,
    connection_id: u64,
    event_state: &GatewayV2EventState,
    pending_client_responses: &mut PendingClientResponses,
    pending_response: PendingClientResponse,
    client_send_timeout: std::time::Duration,
) -> io::Result<()> {
    let pending_response_request_id = pending_response.request_id.clone();
    pending_client_responses.settle_response(&pending_response_request_id);
    update_active_v2_connection_pending_counts(
        connection.observability,
        connection_id,
        event_state,
        &pending_client_responses.active,
    );
    match pending_response.result {
        Ok(Ok(result)) => {
            if let Err(err) = send_jsonrpc(
                socket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: pending_response.request_id.clone(),
                    result,
                }),
                client_send_timeout,
            )
            .await
            {
                let outcome = classify_v2_connection_error(&err);
                observe_client_response_send_failure(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.request_id,
                    &pending_response.method,
                    outcome,
                    &err,
                );
                observe_v2_request(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.method,
                    outcome,
                    pending_response.started_at.elapsed(),
                );
                return Err(err);
            }
            observe_v2_request(
                connection.observability,
                &pending_response.request_context,
                &pending_response.method,
                "ok",
                pending_response.started_at.elapsed(),
            );
        }
        Ok(Err(error)) => {
            let outcome = jsonrpc_error_outcome_code(error.code);
            if let Err(err) = send_jsonrpc_error(
                socket,
                pending_response.request_id.clone(),
                error,
                client_send_timeout,
            )
            .await
            {
                let outcome = classify_v2_connection_error(&err);
                observe_client_response_send_failure(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.request_id,
                    &pending_response.method,
                    outcome,
                    &err,
                );
                observe_v2_request(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.method,
                    outcome,
                    pending_response.started_at.elapsed(),
                );
                return Err(err);
            }
            observe_v2_request(
                connection.observability,
                &pending_response.request_context,
                &pending_response.method,
                outcome,
                pending_response.started_at.elapsed(),
            );
        }
        Err(err) => {
            if let Err(send_err) = send_jsonrpc_error(
                socket,
                pending_response.request_id.clone(),
                JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("gateway upstream request failed: {err}"),
                    data: None,
                },
                client_send_timeout,
            )
            .await
            {
                let outcome = classify_v2_connection_error(&send_err);
                observe_client_response_send_failure(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.request_id,
                    &pending_response.method,
                    outcome,
                    &send_err,
                );
                observe_v2_request(
                    connection.observability,
                    &pending_response.request_context,
                    &pending_response.method,
                    outcome,
                    pending_response.started_at.elapsed(),
                );
                return Err(send_err);
            }
            observe_v2_request(
                connection.observability,
                &pending_response.request_context,
                &pending_response.method,
                "internal_error",
                pending_response.started_at.elapsed(),
            );
        }
    }
    Ok(())
}
