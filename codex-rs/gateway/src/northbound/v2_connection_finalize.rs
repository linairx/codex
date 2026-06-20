use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2EventState;
use crate::northbound::v2_connection::PendingClientResponse;
use crate::northbound::v2_connection::PendingClientResponses;
use crate::northbound::v2_connection_lifecycle::log_aborted_pending_client_requests;
use crate::northbound::v2_connection_lifecycle::log_client_send_timeout;
use crate::northbound::v2_connection_lifecycle::log_downstream_shutdown_failure;
use crate::northbound::v2_connection_lifecycle::observe_aborted_pending_client_requests;
use crate::northbound::v2_connection_runtime::GatewayV2ConnectionRunResult;
use crate::northbound::v2_counts::answered_but_unresolved_server_request_count;
use crate::northbound::v2_counts::pending_client_request_method_counts;
use crate::northbound::v2_counts::pending_client_request_worker_counts;
use crate::northbound::v2_counts::server_request_backlog_method_counts;
use crate::northbound::v2_counts::server_request_backlog_worker_counts;
use crate::northbound::v2_server_request_cleanup::reject_pending_server_requests;
use crate::northbound::v2_server_request_cleanup::should_reject_pending_server_requests_after_connection_error;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use std::io;
use std::io::ErrorKind;
use tokio::sync::mpsc;

pub(crate) struct GatewayV2ConnectionFinalization<'a> {
    pub(crate) downstream: GatewayV2DownstreamRouter,
    pub(crate) connection: &'a GatewayV2ConnectionContext<'a>,
    pub(crate) event_state: GatewayV2EventState,
    pub(crate) pending_client_responses: PendingClientResponses,
    pub(crate) pending_client_response_rx: &'a mut mpsc::Receiver<PendingClientResponse>,
    pub(crate) connection_outcome: &'static str,
    pub(crate) connection_detail: Option<String>,
    pub(crate) reject_pending_server_requests_on_exit: bool,
    pub(crate) loop_result: io::Result<()>,
}

pub(crate) async fn finalize_websocket_connection(
    finalization: GatewayV2ConnectionFinalization<'_>,
) -> io::Result<GatewayV2ConnectionRunResult> {
    let GatewayV2ConnectionFinalization {
        downstream,
        connection,
        mut event_state,
        mut pending_client_responses,
        pending_client_response_rx,
        mut connection_outcome,
        mut connection_detail,
        reject_pending_server_requests_on_exit,
        mut loop_result,
    } = finalization;
    pending_client_responses.settle_completed_responses(pending_client_response_rx);
    if let Err(err) = &loop_result
        && err.kind() == ErrorKind::TimedOut
    {
        connection.observability.record_v2_client_send_timeout();
        log_client_send_timeout(
            connection.request_context,
            err.to_string().as_str(),
            &pending_client_responses.active,
            &event_state.pending_server_requests,
            &event_state.resolved_server_requests,
        );
    }

    let pending_client_request_count = pending_client_responses.count;
    let pending_client_request_worker_counts =
        pending_client_request_worker_counts(&pending_client_responses.active);
    let pending_client_request_method_counts =
        pending_client_request_method_counts(&pending_client_responses.active);
    if !pending_client_responses.active.is_empty() {
        log_aborted_pending_client_requests(
            connection.request_context,
            connection_outcome,
            connection_detail.as_deref(),
            &pending_client_responses.active,
        );
        observe_aborted_pending_client_requests(
            connection.observability,
            connection_outcome,
            &pending_client_responses.active,
        );
    }
    for task in pending_client_responses.tasks {
        task.abort();
    }

    let pending_server_request_count = event_state.pending_server_requests.len();
    let answered_but_unresolved_server_request_count =
        answered_but_unresolved_server_request_count(&event_state.resolved_server_requests);
    let server_request_backlog_worker_counts = server_request_backlog_worker_counts(
        &event_state.pending_server_requests,
        &event_state.resolved_server_requests,
    );
    let server_request_backlog_method_counts = server_request_backlog_method_counts(
        &event_state.pending_server_requests,
        &event_state.resolved_server_requests,
    );

    if (reject_pending_server_requests_on_exit
        || loop_result
            .as_ref()
            .err()
            .is_some_and(should_reject_pending_server_requests_after_connection_error))
        && let Err(err) = reject_pending_server_requests(
            &downstream,
            connection.observability,
            connection.request_context,
            connection_outcome,
            connection_detail.as_deref(),
            &mut event_state.pending_server_requests,
            &event_state.resolved_server_requests,
        )
        .await
    {
        connection_outcome = crate::northbound::v2_wire::classify_v2_connection_error(&err);
        connection_detail = Some(err.to_string());
        loop_result = Err(err);
    }

    let shutdown_result = downstream.shutdown().await;
    let result = match loop_result {
        Ok(()) => {
            if let Err(err) = shutdown_result {
                connection_outcome = crate::northbound::v2_wire::classify_v2_connection_error(&err);
                connection_detail = Some(err.to_string());
                Err(err)
            } else {
                Ok(())
            }
        }
        Err(err) => {
            connection_detail = Some(err.to_string());
            if let Err(shutdown_err) = shutdown_result {
                connection
                    .observability
                    .record_v2_downstream_shutdown_failure(connection_outcome);
                let pending_counts = GatewayV2ConnectionPendingCounts {
                    pending_client_request_count,
                    pending_client_request_worker_counts: pending_client_request_worker_counts
                        .clone(),
                    pending_client_request_method_counts: pending_client_request_method_counts
                        .clone(),
                    pending_server_request_count,
                    answered_but_unresolved_server_request_count,
                    server_request_backlog_worker_counts: server_request_backlog_worker_counts
                        .clone(),
                    server_request_backlog_method_counts: server_request_backlog_method_counts
                        .clone(),
                };
                log_downstream_shutdown_failure(
                    connection.request_context,
                    connection_outcome,
                    connection_detail.as_deref(),
                    &pending_client_responses.active,
                    &pending_counts,
                    &shutdown_err,
                );
            }
            Err(err)
        }
    };
    Ok(GatewayV2ConnectionRunResult {
        outcome: connection_outcome,
        detail: connection_detail,
        pending_client_request_count,
        pending_client_request_worker_counts,
        pending_client_request_method_counts,
        pending_server_request_count,
        answered_but_unresolved_server_request_count,
        server_request_backlog_worker_counts,
        server_request_backlog_method_counts,
        result,
    })
}
