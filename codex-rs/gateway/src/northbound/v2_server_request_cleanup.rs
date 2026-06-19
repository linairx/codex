//! Cleanup helpers for northbound v2 server-request teardown handling.
//!
//! This module owns worker cleanup reporting and pending server-request
//! rejection so `v2.rs` can keep routing and connection orchestration.

use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::PENDING_SERVER_REQUEST_ABORTED_MESSAGE;
use crate::northbound::v2_connection::DownstreamServerRequestKey;
use crate::northbound::v2_connection::PendingServerRequestRoute;
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
use crate::northbound::v2_connection::WorkerServerRequestCleanupReport;
use crate::northbound::v2_routing::worker_for_server_request;
use crate::northbound::v2_server_requests::log_worker_server_request_cleanup;
use crate::northbound::v2_server_requests::publish_worker_server_request_cleanup_event;
use crate::northbound::v2_server_requests::record_client_server_request_cleanup_metrics;
use crate::northbound::v2_server_requests::record_v2_server_request_lifecycle_method_counts;
use crate::northbound::v2_server_requests_logging::log_rejected_pending_server_requests;
use crate::northbound::v2_wire::classify_v2_connection_error;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use tracing::warn;

pub(crate) fn report_worker_server_request_cleanup(
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    report: &WorkerServerRequestCleanupReport<'_>,
    cleanup: &WorkerServerRequestCleanup,
) {
    log_worker_server_request_cleanup(
        worker_id,
        report.worker_websocket_url,
        report.remaining_worker_count,
        report.disconnect_message,
        cleanup,
        report.message,
    );
    publish_worker_server_request_cleanup_event(
        observability,
        worker_id,
        report.worker_websocket_url,
        report.remaining_worker_count,
        report.disconnect_message,
        cleanup,
    );
}

pub(crate) fn record_worker_cleanup_resolution_send_failure(
    observability: &GatewayObservability,
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    resolved_notification: &WorkerCleanupResolvedNotification,
    err: &io::Error,
) {
    let outcome = classify_v2_connection_error(err);
    let notification = &resolved_notification.notification;
    let method = resolved_notification.method.as_str();
    observability.record_v2_notification_send_failure("serverRequest/resolved", outcome);
    observability
        .record_v2_server_request_lifecycle_event("worker_cleanup_resolution_send_failed", method);
    warn!(
        tenant_id = %request_context.tenant_id,
        project_id = ?request_context.project_id,
        worker_id = ?worker_id,
        worker_websocket_url,
        thread_id = notification.thread_id.as_str(),
        gateway_request_id = ?notification.request_id,
        method,
        outcome,
        %err,
        "failed to deliver synthesized serverRequest/resolved during worker cleanup"
    );
}

pub(crate) async fn reject_pending_server_requests(
    downstream: &crate::northbound::v2_connection::GatewayV2DownstreamRouter,
    observability: &GatewayObservability,
    request_context: &GatewayRequestContext,
    connection_outcome: &str,
    connection_detail: Option<&str>,
    pending_server_requests: &mut HashMap<RequestId, PendingServerRequestRoute>,
    resolved_server_requests: &HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
) -> io::Result<()> {
    let mut pending_worker_websocket_urls = pending_server_requests
        .values()
        .map(|route| route.worker_websocket_url.clone())
        .collect::<Vec<_>>();
    pending_worker_websocket_urls.sort();
    pending_worker_websocket_urls.dedup();
    log_rejected_pending_server_requests(
        request_context,
        connection_outcome,
        connection_detail,
        pending_server_requests,
        resolved_server_requests,
        &pending_worker_websocket_urls,
    );
    record_client_server_request_cleanup_metrics(
        observability,
        pending_server_requests,
        resolved_server_requests,
    );

    let mut first_rejection_error = None;
    let mut skipped_unavailable_worker_rejections = BTreeMap::new();
    let mut failed_rejections = BTreeMap::new();
    let mut delivered_rejections = BTreeMap::new();
    for (gateway_request_id, route) in pending_server_requests.drain() {
        let Ok(worker) = worker_for_server_request(downstream, route.worker_id) else {
            *skipped_unavailable_worker_rejections
                .entry(route.method.clone())
                .or_insert(0) += 1;
            observability
                .record_v2_server_request_rejection_delivery_failure(route.method.as_str());
            warn!(
                tenant_id = request_context.tenant_id.as_str(),
                project_id = request_context.project_id.as_deref(),
                worker_id = ?route.worker_id,
                worker_websocket_url = route.worker_websocket_url.as_str(),
                gateway_request_id = ?gateway_request_id,
                downstream_request_id = ?route.downstream_request_id,
                method = route.method.as_str(),
                "skipping pending server-request rejection because the downstream worker route is unavailable"
            );
            continue;
        };
        let worker_websocket_url = worker
            .worker_websocket_url
            .as_deref()
            .unwrap_or("<unknown>");

        match worker
            .request_handle
            .reject_server_request(
                route.downstream_request_id.clone(),
                JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: PENDING_SERVER_REQUEST_ABORTED_MESSAGE.to_string(),
                    data: None,
                },
            )
            .await
        {
            Ok(()) => {
                *delivered_rejections
                    .entry(route.method.clone())
                    .or_insert(0) += 1;
            }
            Err(err) => {
                observability
                    .record_v2_server_request_rejection_delivery_failure(route.method.as_str());
                *failed_rejections.entry(route.method.clone()).or_insert(0) += 1;
                warn!(
                    tenant_id = request_context.tenant_id.as_str(),
                    project_id = request_context.project_id.as_deref(),
                    worker_id = ?route.worker_id,
                    worker_websocket_url,
                    gateway_request_id = ?gateway_request_id,
                    downstream_request_id = ?route.downstream_request_id,
                    method = route.method.as_str(),
                    %err,
                    "failed to reject pending downstream server request during gateway v2 connection cleanup"
                );
                if first_rejection_error.is_none() {
                    first_rejection_error = Some(err);
                }
            }
        }
    }

    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_rejection_delivered",
        &delivered_rejections,
    );
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_rejection_skipped_unavailable_worker",
        &skipped_unavailable_worker_rejections,
    );
    record_v2_server_request_lifecycle_method_counts(
        observability,
        "client_cleanup_rejection_failed",
        &failed_rejections,
    );

    if let Some(err) = first_rejection_error {
        return Err(err);
    }
    Ok(())
}

pub(crate) fn should_reject_pending_server_requests_after_connection_error(
    err: &io::Error,
) -> bool {
    matches!(
        err.kind(),
        ErrorKind::InvalidData
            | ErrorKind::TimedOut
            | ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::UnexpectedEof
            | ErrorKind::WriteZero
            | ErrorKind::NotConnected
    )
}
