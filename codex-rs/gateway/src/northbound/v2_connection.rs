use crate::admission::GatewayAdmissionController;
use crate::api::GatewayV2PendingClientRequestMethodCounts;
use crate::api::GatewayV2PendingClientRequestWorkerCounts;
use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use crate::northbound::v2_counts::answered_but_unresolved_server_request_count;
use crate::northbound::v2_counts::pending_client_request_method_counts;
use crate::northbound::v2_counts::pending_client_request_worker_counts;
use crate::northbound::v2_counts::server_request_backlog_method_counts;
use crate::northbound::v2_counts::server_request_backlog_worker_counts;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2_connection_health::GatewayV2ConnectionPendingCounts;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub(crate) struct GatewayV2ConnectionContext<'a> {
    pub(crate) admission: &'a GatewayAdmissionController,
    pub(crate) observability: &'a GatewayObservability,
    pub(crate) scope_registry: &'a GatewayScopeRegistry,
    pub(crate) request_context: &'a GatewayRequestContext,
    pub(crate) client_send_timeout: Duration,
    pub(crate) max_pending_server_requests: usize,
    pub(crate) max_pending_client_requests: usize,
    pub(crate) opt_out_notification_methods: HashSet<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GatewayV2ConnectionClose {
    pub(crate) outcome: &'static str,
    pub(crate) reject_pending_server_requests: bool,
}

pub(crate) struct GatewayV2EventState {
    pub(crate) pending_server_requests: HashMap<RequestId, PendingServerRequestRoute>,
    pub(crate) resolved_server_requests:
        HashMap<DownstreamServerRequestKey, ResolvedServerRequestRoute>,
    pub(crate) skills_changed_pending_refresh: bool,
    pub(crate) forwarded_connection_notifications:
        HashMap<String, VecDeque<ForwardedConnectionNotification>>,
}

pub(crate) struct ForwardedConnectionNotification {
    pub(crate) worker_id: Option<usize>,
    pub(crate) params: Option<Value>,
}

pub(crate) fn update_active_v2_connection_pending_counts(
    observability: &GatewayObservability,
    connection_id: u64,
    event_state: &GatewayV2EventState,
    pending_client_requests: &HashMap<RequestId, PendingClientRequestRoute>,
) {
    observability
        .v2_connection_health()
        .update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: pending_client_requests.len(),
                pending_client_request_worker_counts: pending_client_request_worker_counts(
                    pending_client_requests,
                ),
                pending_client_request_method_counts: pending_client_request_method_counts(
                    pending_client_requests,
                ),
                pending_server_request_count: event_state.pending_server_requests.len(),
                answered_but_unresolved_server_request_count:
                    answered_but_unresolved_server_request_count(
                        &event_state.resolved_server_requests,
                    ),
                server_request_backlog_worker_counts: server_request_backlog_worker_counts(
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                ),
                server_request_backlog_method_counts: server_request_backlog_method_counts(
                    &event_state.pending_server_requests,
                    &event_state.resolved_server_requests,
                ),
            },
        );
}

pub(crate) struct GatewayV2ConnectionRunResult {
    pub(crate) outcome: &'static str,
    pub(crate) detail: Option<String>,
    pub(crate) pending_client_request_count: usize,
    pub(crate) pending_client_request_worker_counts: Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub(crate) pending_client_request_method_counts: Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub(crate) pending_server_request_count: usize,
    pub(crate) answered_but_unresolved_server_request_count: usize,
    pub(crate) server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub(crate) server_request_backlog_method_counts: Vec<GatewayV2ServerRequestBacklogMethodCounts>,
    pub(crate) result: io::Result<()>,
}

pub(crate) struct PendingClientResponse {
    pub(crate) request_id: RequestId,
    pub(crate) method: String,
    pub(crate) request_context: GatewayRequestContext,
    pub(crate) started_at: Instant,
    pub(crate) result: io::Result<Result<Value, JSONRPCErrorError>>,
}

pub(crate) struct PendingClientRequestRoute {
    pub(crate) method: String,
    pub(crate) request_context: GatewayRequestContext,
    pub(crate) worker_id: Option<usize>,
    pub(crate) worker_websocket_url: String,
    pub(crate) started_at: Instant,
}

pub(crate) struct PendingClientResponses {
    pub(crate) tx: mpsc::Sender<PendingClientResponse>,
    pub(crate) tasks: Vec<JoinHandle<()>>,
    pub(crate) count: usize,
    pub(crate) active: HashMap<RequestId, PendingClientRequestRoute>,
}

impl PendingClientResponses {
    pub(crate) fn settle_response(&mut self, request_id: &RequestId) {
        self.count = self.count.saturating_sub(1);
        self.active.remove(request_id);
        self.tasks.retain(|task| !task.is_finished());
    }

    pub(crate) fn settle_completed_responses(
        &mut self,
        rx: &mut mpsc::Receiver<PendingClientResponse>,
    ) {
        while let Ok(response) = rx.try_recv() {
            self.settle_response(&response.request_id);
        }
    }
}

#[derive(Clone)]
pub(crate) struct GatewayV2ReconnectState {
    pub(crate) configured_worker_ids: Vec<usize>,
    pub(crate) worker_websocket_urls: Vec<String>,
    pub(crate) session_factory: crate::v2::GatewayV2SessionFactory,
    pub(crate) initialize_params: InitializeParams,
    pub(crate) request_context: GatewayRequestContext,
    pub(crate) retry_backoff: Duration,
}

#[derive(Clone)]
pub(crate) struct DownstreamWorkerHandle {
    pub(crate) worker_id: Option<usize>,
    pub(crate) worker_websocket_url: Option<String>,
    pub(crate) request_handle: codex_app_server_client::AppServerRequestHandle,
}

pub(crate) struct DownstreamWorkerEvent {
    pub(crate) worker_id: Option<usize>,
    pub(crate) event: Option<codex_app_server_client::AppServerEvent>,
}

#[derive(Clone)]
pub(crate) struct PendingServerRequestRoute {
    pub(crate) worker_id: Option<usize>,
    pub(crate) worker_websocket_url: String,
    pub(crate) downstream_request_id: RequestId,
    pub(crate) method: String,
    pub(crate) thread_id: Option<String>,
}

pub(crate) struct GatewayRejectedServerRequest<'a> {
    pub(crate) worker_id: Option<usize>,
    pub(crate) worker_websocket_url: &'a str,
    pub(crate) gateway_request_id: &'a RequestId,
    pub(crate) method: &'a str,
    pub(crate) downstream_request_id: RequestId,
}

pub(crate) enum ClientServerRequestAnswer {
    Response(Value),
    Error(JSONRPCErrorError),
}

impl ClientServerRequestAnswer {
    pub(crate) fn method_tag(&self) -> &'static str {
        match self {
            Self::Response(_) => "response",
            Self::Error(_) => "error",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct DownstreamServerRequestKey {
    pub(crate) worker_id: Option<usize>,
    pub(crate) request_id: RequestId,
}

#[derive(Clone)]
pub(crate) struct ResolvedServerRequestRoute {
    pub(crate) gateway_request_id: RequestId,
    pub(crate) worker_websocket_url: String,
    pub(crate) method: String,
    pub(crate) thread_id: Option<String>,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct WorkerServerRequestCleanup {
    pub(crate) resolved_notifications: Vec<WorkerCleanupResolvedNotification>,
    pub(crate) resolved_thread_scoped_requests: usize,
    pub(crate) resolved_thread_scoped_request_ids: Vec<RequestId>,
    pub(crate) resolved_thread_scoped_downstream_request_ids: Vec<RequestId>,
    pub(crate) resolved_thread_scoped_methods: Vec<String>,
    pub(crate) resolved_thread_scoped_thread_ids: Vec<String>,
    pub(crate) stranded_connection_scoped_requests: usize,
    pub(crate) stranded_connection_scoped_request_ids: Vec<RequestId>,
    pub(crate) stranded_connection_scoped_downstream_request_ids: Vec<RequestId>,
    pub(crate) stranded_connection_scoped_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WorkerCleanupResolvedNotification {
    pub(crate) notification: codex_app_server_protocol::ServerRequestResolvedNotification,
    pub(crate) method: String,
}

pub(crate) struct WorkerServerRequestCleanupReport<'a> {
    pub(crate) worker_websocket_url: &'a str,
    pub(crate) remaining_worker_count: usize,
    pub(crate) disconnect_message: Option<&'a str>,
    pub(crate) message: &'a str,
}

impl WorkerServerRequestCleanup {
    pub(crate) fn has_stranded_connection_scoped_requests(&self) -> bool {
        self.stranded_connection_scoped_requests > 0
    }

    pub(crate) fn has_cleaned_up_requests(&self) -> bool {
        self.resolved_thread_scoped_requests > 0 || self.stranded_connection_scoped_requests > 0
    }
}

pub(crate) struct GatewayV2DownstreamRouter {
    pub(crate) workers: Vec<DownstreamWorkerHandle>,
    pub(crate) event_tx: mpsc::Sender<DownstreamWorkerEvent>,
    pub(crate) event_rx: mpsc::Receiver<DownstreamWorkerEvent>,
    pub(crate) shutdown_txs: Vec<tokio::sync::oneshot::Sender<()>>,
    pub(crate) event_tasks: Vec<JoinHandle<io::Result<()>>>,
    pub(crate) next_worker: usize,
    pub(crate) initialized_notification_sent: bool,
    pub(crate) active_fs_watches: HashMap<String, codex_app_server_protocol::FsWatchParams>,
    pub(crate) reconnect_retry_after: HashMap<usize, Instant>,
    pub(crate) reconnect_state: Option<GatewayV2ReconnectState>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UnavailableWorkerRouteDiagnostics {
    pub(crate) worker_id: usize,
    pub(crate) websocket_url: String,
    pub(crate) reconnect_backoff_active: bool,
    pub(crate) reconnect_backoff_remaining_seconds: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct FailClosedMultiWorkerRouteError {
    pub(crate) message: String,
}

impl std::fmt::Display for FailClosedMultiWorkerRouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FailClosedMultiWorkerRouteError {}
