use crate::admission::GatewayAdmissionController;
use crate::auth::GatewayAuth;
use crate::observability::GatewayObservability;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
use crate::northbound::v2_aggregation::*;
#[cfg(test)]
use crate::northbound::v2_connection::ClientServerRequestAnswer;
#[cfg(test)]
use crate::northbound::v2_connection::DownstreamServerRequestKey;
#[cfg(test)]
use crate::northbound::v2_connection::DownstreamWorkerEvent;
#[cfg(test)]
use crate::northbound::v2_connection::DownstreamWorkerHandle;
#[cfg(test)]
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
#[cfg(test)]
use crate::northbound::v2_connection::GatewayRejectedServerRequest;
#[cfg(test)]
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
#[cfg(test)]
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
#[cfg(test)]
use crate::northbound::v2_connection::GatewayV2EventState;
#[cfg(test)]
use crate::northbound::v2_connection::GatewayV2ReconnectState;
#[cfg(test)]
use crate::northbound::v2_connection::PendingClientRequestRoute;
#[cfg(test)]
use crate::northbound::v2_connection::PendingClientResponse;
#[cfg(test)]
use crate::northbound::v2_connection::PendingClientResponses;
#[cfg(test)]
use crate::northbound::v2_connection::PendingServerRequestRoute;
#[cfg(test)]
use crate::northbound::v2_connection::ResolvedServerRequestRoute;
#[cfg(test)]
use crate::northbound::v2_connection::UnavailableWorkerRouteDiagnostics;
#[cfg(test)]
pub(crate) use crate::northbound::v2_connection::WorkerCleanupResolvedNotification;
#[cfg(test)]
use crate::northbound::v2_connection::WorkerServerRequestCleanup;
#[cfg(test)]
use crate::northbound::v2_connection::WorkerServerRequestCleanupReport;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_aborted_pending_client_requests;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_client_send_timeout;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_downstream_backpressure_close;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_downstream_shutdown_failure;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_duplicate_pending_client_request;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_rejected_hidden_downstream_server_request;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_rejected_saturated_client_request;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_rejected_saturated_server_request;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::log_unexpected_client_server_request_response;
#[cfg(test)]
use crate::northbound::v2_connection_lifecycle::observe_aborted_pending_client_requests;
#[cfg(test)]
pub(crate) use crate::northbound::v2_connection_runtime::deliver_client_server_request_answer;
#[cfg(test)]
use crate::northbound::v2_connection_runtime::log_fail_closed_multi_worker_request;
#[cfg(test)]
pub(crate) use crate::northbound::v2_connection_runtime::reject_downstream_server_request_at_gateway_boundary;
#[cfg(test)]
use crate::northbound::v2_counts::server_request_backlog_method_counts;
#[cfg(test)]
use crate::northbound::v2_counts::server_request_backlog_worker_counts;
#[cfg(test)]
use crate::northbound::v2_notifications::MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD;
#[cfg(test)]
pub(crate) use crate::northbound::v2_notifications::forwarded_connection_notification_duplicate;
#[cfg(test)]
pub(crate) use crate::northbound::v2_notifications::log_suppressed_duplicate_connection_notification;
#[cfg(test)]
pub(crate) use crate::northbound::v2_notifications::log_suppressed_hidden_thread_notification;
#[cfg(test)]
pub(crate) use crate::northbound::v2_notifications::log_suppressed_opted_out_notification;
#[cfg(test)]
pub(crate) use crate::northbound::v2_notifications::log_suppressed_skills_changed_notification;
#[cfg(test)]
use crate::northbound::v2_notifications::record_forwarded_connection_notification;
#[cfg(test)]
use crate::northbound::v2_notifications::should_deduplicate_connection_notification;
#[cfg(test)]
pub(crate) use crate::northbound::v2_request_routing_handoff::recover_visible_thread_worker_route;
#[cfg(test)]
use crate::northbound::v2_routing::worker_for_notification;
#[cfg(test)]
use crate::northbound::v2_routing::worker_for_request;
#[cfg(test)]
pub(crate) use crate::northbound::v2_scope::log_failed_visible_thread_worker_route_recovery;
#[cfg(test)]
pub(crate) use crate::northbound::v2_scope::log_recovered_visible_thread_worker_route;
#[cfg(test)]
pub(crate) use crate::northbound::v2_scope::response_thread_id;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_request_cleanup::record_worker_cleanup_resolution_send_failure;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_request_cleanup::reject_pending_server_requests;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_request_cleanup::report_worker_server_request_cleanup;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_request_cleanup::should_reject_pending_server_requests_after_connection_error;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests::collect_server_request_cleanup_for_worker;
#[cfg(test)]
use crate::northbound::v2_server_requests::log_duplicate_downstream_server_request;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests::log_worker_server_request_cleanup;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests::publish_worker_server_request_cleanup_event;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests::record_client_server_request_cleanup_metrics;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests::record_worker_server_request_cleanup_metrics;
#[cfg(test)]
pub(crate) use crate::northbound::v2_server_requests_logging::log_rejected_pending_server_requests;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::INVALID_CLIENT_UTF8_PAYLOAD_CLOSE_REASON;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::classify_v2_connection_error;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_client_response_send_failure;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_downstream_connect_protocol_violation;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_downstream_protocol_violation;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_downstream_reconnect_protocol_violation;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_downstream_server_request_forward_failure;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::log_notification_send_failure;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::observe_v2_connection;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::observe_v2_request;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::server_notification_to_jsonrpc;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire::tagged_type_to_notification;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::await_io_with_timeout;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::log_close_frame_send_failure;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::send_client_jsonrpc;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::send_client_jsonrpc_error;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::send_observed_close_frame;
#[cfg(test)]
pub(crate) use crate::northbound::v2_wire_send::send_observed_invalid_payload_close;
#[cfg(test)]
pub(crate) use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;

pub(crate) const INVALID_REQUEST_CODE: i64 = -32600;
pub(crate) const INVALID_PARAMS_CODE: i64 = -32602;
pub(crate) const INTERNAL_ERROR_CODE: i64 = -32603;
pub(crate) const DOWNSTREAM_SESSION_ENDED_CLOSE_REASON: &str =
    "downstream app-server session ended";
pub(crate) const INITIALIZE_TIMEOUT_CLOSE_REASON: &str = "initialize request timed out";
pub(crate) const UNEXPECTED_CLIENT_SERVER_REQUEST_RESPONSE_CLOSE_REASON: &str =
    "unexpected gateway websocket server-request response";
pub(crate) const DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON: &str =
    "duplicate pending client request id";
pub(crate) const DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON: &str =
    "duplicate downstream server-request id";
pub(crate) const STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON: &str =
    "downstream worker disconnected during connection-scoped server request";
pub(crate) const PENDING_SERVER_REQUEST_ABORTED_MESSAGE: &str =
    "gateway websocket connection ended before server request was resolved";
const MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION: usize = 64;
const MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION: usize = 64;
#[derive(Clone, Copy, Debug)]
pub struct GatewayV2Timeouts {
    pub initialize: Duration,
    pub client_send: Duration,
    pub reconnect_retry_backoff: Duration,
    pub max_pending_server_requests: usize,
    pub max_pending_client_requests: usize,
}

impl Default for GatewayV2Timeouts {
    fn default() -> Self {
        Self {
            initialize: Duration::from_secs(30),
            client_send: Duration::from_secs(10),
            reconnect_retry_backoff: Duration::from_secs(1),
            max_pending_server_requests: MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
            max_pending_client_requests: MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
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

pub async fn websocket_upgrade_handler(
    websocket: WebSocketUpgrade,
    State(state): State<GatewayV2State>,
    headers: HeaderMap,
) -> impl IntoResponse {
    crate::northbound::v2_connection_loop::websocket_upgrade_handler(
        websocket,
        State(state),
        headers,
    )
    .await
}

#[cfg(test)]
pub(crate) use crate::northbound::v2_request_dispatch::handle_client_request;

#[cfg(test)]
pub(crate) use crate::northbound::v2_connection_event::handle_app_server_event;

#[cfg(test)]
#[path = "v2_tests.rs"]
mod tests;
