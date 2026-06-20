use crate::northbound::v2::DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_connection::GatewayRejectedServerRequest;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection_lifecycle::log_rejected_hidden_downstream_server_request;
use crate::northbound::v2_connection_lifecycle::log_rejected_saturated_server_request;
use crate::northbound::v2_connection_runtime::reject_downstream_server_request_at_gateway_boundary;
use crate::northbound::v2_scope::request_thread_id;
use crate::northbound::v2_scope::server_request_visible_to;
use crate::northbound::v2_server_requests::log_duplicate_downstream_server_request;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::northbound::v2_wire::hidden_thread_error_message;
use crate::northbound::v2_wire::log_downstream_server_request_forward_failure;
use crate::northbound::v2_wire::server_request_to_jsonrpc;
use crate::northbound::v2_wire_send::send_jsonrpc;
use crate::northbound::v2_wire_send::send_observed_close_frame;
use crate::v2::GatewayV2SessionFactory;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::ServerRequest;
use std::io;

pub(crate) async fn handle_server_request_event(
    socket: &mut WebSocket,
    downstream: &mut GatewayV2DownstreamRouter,
    session_factory: &GatewayV2SessionFactory,
    connection: &GatewayV2ConnectionContext<'_>,
    event_state: &mut GatewayV2EventState,
    worker_id: Option<usize>,
    request: ServerRequest,
) -> io::Result<()> {
    let worker_websocket_url = downstream
        .websocket_url_for_worker_id(worker_id)
        .to_string();
    let gateway_request_id = if downstream.multi_worker_topology() {
        session_factory.next_server_request_id()
    } else {
        request.id().clone()
    };
    let (request, downstream_request_id) = server_request_to_jsonrpc(request, gateway_request_id)?;
    if event_state
        .pending_server_requests
        .contains_key(&request.id)
    {
        log_duplicate_downstream_server_request(
            connection.request_context,
            worker_id,
            worker_websocket_url.as_str(),
            &request.id,
            &request.method,
            &event_state.pending_server_requests,
        );
        connection
            .observability
            .record_v2_server_request_lifecycle_event("duplicate_pending_request", &request.method);
        send_observed_close_frame(
            socket,
            connection.observability,
            connection.request_context,
            close_code::ERROR,
            DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON,
            connection.client_send_timeout,
        )
        .await?;
        return Ok(());
    }
    if server_request_visible_to(
        connection.scope_registry,
        connection.request_context,
        &request,
    ) {
        if let Some(error) = crate::northbound::v2_limits::pending_server_request_limit_error(
            event_state.pending_server_requests.len(),
            connection.max_pending_server_requests,
        ) {
            log_rejected_saturated_server_request(
                connection.request_context,
                worker_id,
                worker_websocket_url.as_str(),
                &request.id,
                &request.method,
                &event_state.pending_server_requests,
                connection.max_pending_server_requests,
            );
            connection
                .observability
                .record_v2_server_request_rejection(&request.method, "pending_limit");
            connection
                .observability
                .record_v2_server_request_lifecycle_event(
                    "downstream_server_request_rejected_pending_limit",
                    &request.method,
                );
            reject_downstream_server_request_at_gateway_boundary(
                downstream,
                connection,
                GatewayRejectedServerRequest {
                    worker_id,
                    worker_websocket_url: worker_websocket_url.as_str(),
                    gateway_request_id: &request.id,
                    method: &request.method,
                    downstream_request_id,
                },
                error,
            )
            .await?;
            return Ok(());
        }
        event_state.pending_server_requests.insert(
            request.id.clone(),
            PendingServerRequestRoute {
                worker_id,
                worker_websocket_url: worker_websocket_url.to_string(),
                downstream_request_id,
                method: request.method.clone(),
                thread_id: request_thread_id(&request).map(str::to_string),
            },
        );
        let method = request.method.clone();
        if let Err(err) = send_jsonrpc(
            socket,
            JSONRPCMessage::Request(request),
            connection.client_send_timeout,
        )
        .await
        {
            let outcome = classify_v2_connection_error(&err);
            log_downstream_server_request_forward_failure(
                connection.request_context,
                worker_id,
                worker_websocket_url.as_str(),
                &method,
                outcome,
                &err,
            );
            connection
                .observability
                .record_v2_server_request_forward_send_failure(&method, outcome);
            connection
                .observability
                .record_v2_server_request_lifecycle_event(
                    "downstream_server_request_forward_delivery_failed",
                    &method,
                );
            return Err(err);
        }
        connection
            .observability
            .record_v2_server_request_lifecycle_event(
                "downstream_server_request_forwarded",
                &method,
            );
    } else {
        log_rejected_hidden_downstream_server_request(
            connection.request_context,
            worker_id,
            worker_websocket_url.as_str(),
            &request.id,
            &request.method,
            request_thread_id(&request),
        );
        connection
            .observability
            .record_v2_server_request_rejection(&request.method, "hidden_thread");
        connection
            .observability
            .record_v2_server_request_lifecycle_event(
                "downstream_server_request_rejected_hidden_thread",
                &request.method,
            );
        let message = hidden_thread_error_message(&request).to_string();
        reject_downstream_server_request_at_gateway_boundary(
            downstream,
            connection,
            GatewayRejectedServerRequest {
                worker_id,
                worker_websocket_url: worker_websocket_url.as_str(),
                gateway_request_id: &request.id,
                method: &request.method,
                downstream_request_id,
            },
            JSONRPCErrorError {
                code: INVALID_PARAMS_CODE,
                message,
                data: None,
            },
        )
        .await?;
    }

    Ok(())
}
