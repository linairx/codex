use codex_app_server_protocol::AdditionalPermissionProfile;
use codex_app_server_protocol::CommandAction;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::ExecPolicyAmendment;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalParams;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::GrantedPermissionProfile;
use codex_app_server_protocol::NetworkApprovalContext;
use codex_app_server_protocol::NetworkPolicyAmendment;
use codex_app_server_protocol::PermissionGrantScope;
use codex_app_server_protocol::PermissionsRequestApprovalParams;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::RequestPermissionProfile;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SortDirection;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadListResponse as AppServerThreadListResponse;
use codex_app_server_protocol::ThreadSortKey;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputParams;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayHealthStatus {
    Ok,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayExecutionMode {
    InProcess,
    ExecServer,
    WorkerManaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayV2CompatibilityMode {
    Embedded,
    RemoteSingleWorker,
    RemoteMultiWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRemoteWorkerHealth {
    pub worker_id: usize,
    pub websocket_url: String,
    pub account_id: Option<String>,
    pub account_capacity: GatewayAccountCapacityStatus,
    pub account_capacity_reason: Option<String>,
    pub account_capacity_last_changed_at: Option<i64>,
    pub healthy: bool,
    pub reconnecting: bool,
    pub reconnect_attempt_count: u32,
    pub last_error: Option<String>,
    pub last_state_change_at: Option<i64>,
    pub last_error_at: Option<i64>,
    pub next_reconnect_at: Option<i64>,
    pub reconnect_backoff_remaining_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayProjectWorkerRoute {
    pub tenant_id: String,
    pub project_id: String,
    pub worker_id: usize,
    pub account_id: Option<String>,
    pub account_capacity: GatewayAccountCapacityStatus,
    pub worker_healthy: bool,
    pub account_routing_eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayAccountCapacityStatus {
    Available,
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2TransportConfig {
    pub initialize_timeout_seconds: u64,
    pub client_send_timeout_seconds: u64,
    pub reconnect_retry_backoff_seconds: u64,
    pub max_pending_server_requests: usize,
    pub max_pending_client_requests: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ConnectionHealth {
    pub active_connection_count: usize,
    pub active_connection_pending_client_request_count: usize,
    pub active_connection_max_pending_client_request_count: usize,
    pub active_connection_peak_pending_client_request_count: usize,
    pub active_connection_pending_client_request_started_at: Option<i64>,
    pub active_connection_pending_client_request_worker_counts:
        Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub active_connection_pending_client_request_method_counts:
        Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub active_connection_pending_server_request_count: usize,
    pub active_connection_answered_but_unresolved_server_request_count: usize,
    pub active_connection_server_request_backlog_count: usize,
    pub active_connection_max_server_request_backlog_count: usize,
    pub active_connection_peak_server_request_backlog_count: usize,
    pub active_connection_server_request_backlog_started_at: Option<i64>,
    pub active_connection_server_request_backlog_worker_counts:
        Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub active_connection_server_request_backlog_method_counts:
        Vec<GatewayV2ServerRequestBacklogMethodCounts>,
    pub project_worker_route_selection_count: usize,
    pub project_worker_route_selection_worker_counts:
        Vec<GatewayV2ProjectWorkerRouteSelectionWorkerCounts>,
    pub last_project_worker_route_selected_worker_id: Option<usize>,
    pub last_project_worker_route_selected_tenant_id: Option<String>,
    pub last_project_worker_route_selected_project_id: Option<String>,
    pub last_project_worker_route_selected_thread_id: Option<String>,
    pub last_project_worker_route_selected_account_id: Option<String>,
    pub last_project_worker_route_selected_at: Option<i64>,
    pub account_capacity_event_counts: BTreeMap<String, usize>,
    pub account_capacity_event_worker_counts: Vec<GatewayV2AccountCapacityWorkerEventCounts>,
    pub last_account_capacity_event: Option<String>,
    pub last_account_capacity_event_worker_id: Option<usize>,
    pub last_account_capacity_event_tenant_id: Option<String>,
    pub last_account_capacity_event_project_id: Option<String>,
    pub last_account_capacity_event_reason: Option<String>,
    pub last_account_capacity_event_at: Option<i64>,
    pub worker_reconnect_event_counts: BTreeMap<String, usize>,
    pub worker_reconnect_event_worker_counts: Vec<GatewayV2WorkerReconnectWorkerEventCounts>,
    pub last_worker_reconnect_event: Option<String>,
    pub last_worker_reconnect_event_worker_id: Option<usize>,
    pub last_worker_reconnect_event_at: Option<i64>,
    pub request_counts: Vec<GatewayV2RequestCounts>,
    pub last_request_method: Option<String>,
    pub last_request_outcome: Option<String>,
    pub last_request_duration_ms: Option<u64>,
    pub max_request_duration_ms: Option<u64>,
    pub last_request_at: Option<i64>,
    pub client_request_rejection_counts: Vec<GatewayV2ClientRequestRejectionCounts>,
    pub last_client_request_rejection_method: Option<String>,
    pub last_client_request_rejection_reason: Option<String>,
    pub last_client_request_rejection_at: Option<i64>,
    pub server_request_rejection_counts: Vec<GatewayV2ServerRequestRejectionCounts>,
    pub last_server_request_rejection_method: Option<String>,
    pub last_server_request_rejection_reason: Option<String>,
    pub last_server_request_rejection_at: Option<i64>,
    pub server_request_lifecycle_event_counts: Vec<GatewayV2ServerRequestLifecycleEventCounts>,
    pub last_server_request_lifecycle_event: Option<String>,
    pub last_server_request_lifecycle_method: Option<String>,
    pub last_server_request_lifecycle_at: Option<i64>,
    pub fail_closed_request_counts: Vec<GatewayV2FailClosedRequestCounts>,
    pub last_fail_closed_request_method: Option<String>,
    pub last_fail_closed_request_reconnect_backoff_active: Option<bool>,
    pub last_fail_closed_request_at: Option<i64>,
    pub upstream_request_failure_counts: Vec<GatewayV2UpstreamRequestFailureCounts>,
    pub last_upstream_request_failure_method: Option<String>,
    pub last_upstream_request_failure_reconnect_backoff_active: Option<bool>,
    pub last_upstream_request_failure_at: Option<i64>,
    pub downstream_backpressure_counts: Vec<GatewayV2DownstreamBackpressureCounts>,
    pub last_downstream_backpressure_worker_id: Option<usize>,
    pub last_downstream_backpressure_at: Option<i64>,
    pub client_send_timeout_count: usize,
    pub last_client_send_timeout_at: Option<i64>,
    pub thread_list_deduplication_counts: Vec<GatewayV2ThreadListDeduplicationCounts>,
    pub last_thread_list_deduplication_selected_worker_id: Option<usize>,
    pub last_thread_list_deduplication_at: Option<i64>,
    pub thread_route_recovery_counts: Vec<GatewayV2ThreadRouteRecoveryCounts>,
    pub last_thread_route_recovery_outcome: Option<String>,
    pub last_thread_route_recovery_at: Option<i64>,
    pub degraded_thread_discovery_counts: Vec<GatewayV2DegradedThreadDiscoveryCounts>,
    pub last_degraded_thread_discovery_method: Option<String>,
    pub last_degraded_thread_discovery_reconnect_backoff_active: Option<bool>,
    pub last_degraded_thread_discovery_at: Option<i64>,
    pub forwarded_notification_counts: Vec<GatewayV2ForwardedNotificationCounts>,
    pub last_forwarded_notification_method: Option<String>,
    pub last_forwarded_notification_at: Option<i64>,
    pub notification_send_failure_counts: Vec<GatewayV2NotificationSendFailureCounts>,
    pub last_notification_send_failure_method: Option<String>,
    pub last_notification_send_failure_outcome: Option<String>,
    pub last_notification_send_failure_at: Option<i64>,
    pub client_response_send_failure_counts: Vec<GatewayV2ClientResponseSendFailureCounts>,
    pub last_client_response_send_failure_method: Option<String>,
    pub last_client_response_send_failure_outcome: Option<String>,
    pub last_client_response_send_failure_at: Option<i64>,
    pub downstream_shutdown_failure_counts: Vec<GatewayV2DownstreamShutdownFailureCounts>,
    pub last_downstream_shutdown_failure_outcome: Option<String>,
    pub last_downstream_shutdown_failure_at: Option<i64>,
    pub close_frame_send_failure_counts: Vec<GatewayV2CloseFrameSendFailureCounts>,
    pub last_close_frame_send_failure_code: Option<u16>,
    pub last_close_frame_send_failure_outcome: Option<String>,
    pub last_close_frame_send_failure_at: Option<i64>,
    pub server_request_forward_send_failure_counts:
        Vec<GatewayV2ServerRequestForwardSendFailureCounts>,
    pub last_server_request_forward_send_failure_method: Option<String>,
    pub last_server_request_forward_send_failure_outcome: Option<String>,
    pub last_server_request_forward_send_failure_at: Option<i64>,
    pub server_request_answer_delivery_failure_counts:
        Vec<GatewayV2ServerRequestAnswerDeliveryFailureCounts>,
    pub last_server_request_answer_delivery_failure_response_kind: Option<String>,
    pub last_server_request_answer_delivery_failure_at: Option<i64>,
    pub server_request_rejection_delivery_failure_counts:
        Vec<GatewayV2ServerRequestRejectionDeliveryFailureCounts>,
    pub last_server_request_rejection_delivery_failure_method: Option<String>,
    pub last_server_request_rejection_delivery_failure_at: Option<i64>,
    pub suppressed_notification_counts: Vec<GatewayV2SuppressedNotificationCounts>,
    pub last_suppressed_notification_method: Option<String>,
    pub last_suppressed_notification_reason: Option<String>,
    pub last_suppressed_notification_at: Option<i64>,
    pub protocol_violation_counts: Vec<GatewayV2ProtocolViolationCounts>,
    pub protocol_violation_worker_counts: Vec<GatewayV2ProtocolViolationWorkerCounts>,
    pub last_protocol_violation_phase: Option<String>,
    pub last_protocol_violation_reason: Option<String>,
    pub last_protocol_violation_worker_id: Option<usize>,
    pub last_protocol_violation_at: Option<i64>,
    pub connection_outcome_counts: Vec<GatewayV2ConnectionOutcomeCounts>,
    pub peak_active_connection_count: usize,
    pub total_connection_count: u64,
    pub last_connection_started_at: Option<i64>,
    pub last_connection_completed_at: Option<i64>,
    pub last_connection_duration_ms: Option<u64>,
    pub max_connection_duration_ms: Option<u64>,
    pub last_connection_outcome: Option<String>,
    pub last_connection_detail: Option<String>,
    pub last_connection_pending_client_request_count: usize,
    pub last_connection_max_pending_client_request_count: usize,
    pub last_connection_pending_client_request_started_at: Option<i64>,
    pub last_connection_pending_client_request_worker_counts:
        Vec<GatewayV2PendingClientRequestWorkerCounts>,
    pub last_connection_pending_client_request_method_counts:
        Vec<GatewayV2PendingClientRequestMethodCounts>,
    pub last_connection_pending_server_request_count: usize,
    pub last_connection_answered_but_unresolved_server_request_count: usize,
    pub last_connection_server_request_backlog_count: usize,
    pub last_connection_max_server_request_backlog_count: usize,
    pub last_connection_server_request_backlog_started_at: Option<i64>,
    pub last_connection_server_request_backlog_worker_counts:
        Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
    pub last_connection_server_request_backlog_method_counts:
        Vec<GatewayV2ServerRequestBacklogMethodCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2PendingClientRequestWorkerCounts {
    pub worker_id: Option<usize>,
    pub pending_client_request_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2PendingClientRequestMethodCounts {
    pub method: String,
    pub pending_client_request_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2AccountCapacityWorkerEventCounts {
    pub worker_id: usize,
    pub event_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2WorkerReconnectWorkerEventCounts {
    pub worker_id: usize,
    pub event_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2RequestCounts {
    pub method: String,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ClientRequestRejectionCounts {
    pub method: String,
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestRejectionCounts {
    pub method: String,
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestLifecycleEventCounts {
    pub event: String,
    pub method: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2FailClosedRequestCounts {
    pub method: String,
    pub reconnect_backoff_active: bool,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2UpstreamRequestFailureCounts {
    pub method: String,
    pub reconnect_backoff_active: bool,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2DownstreamBackpressureCounts {
    pub worker_id: Option<usize>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ThreadListDeduplicationCounts {
    pub selected_worker_id: Option<usize>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ThreadRouteRecoveryCounts {
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2DegradedThreadDiscoveryCounts {
    pub method: String,
    pub reconnect_backoff_active: bool,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2SuppressedNotificationCounts {
    pub method: String,
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ForwardedNotificationCounts {
    pub method: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2NotificationSendFailureCounts {
    pub method: String,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ClientResponseSendFailureCounts {
    pub method: String,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2DownstreamShutdownFailureCounts {
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2CloseFrameSendFailureCounts {
    pub code: u16,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestForwardSendFailureCounts {
    pub method: String,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestAnswerDeliveryFailureCounts {
    pub response_kind: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestRejectionDeliveryFailureCounts {
    pub method: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ProtocolViolationCounts {
    pub phase: String,
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ProtocolViolationWorkerCounts {
    pub worker_id: usize,
    pub violation_counts: Vec<GatewayV2ProtocolViolationCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ConnectionOutcomeCounts {
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestBacklogWorkerCounts {
    pub worker_id: Option<usize>,
    pub pending_server_request_count: usize,
    pub answered_but_unresolved_server_request_count: usize,
    pub server_request_backlog_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ServerRequestBacklogMethodCounts {
    pub method: String,
    pub pending_server_request_count: usize,
    pub answered_but_unresolved_server_request_count: usize,
    pub server_request_backlog_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
    pub worker_id: usize,
    pub project_worker_route_selection_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayHealthResponse {
    pub status: GatewayHealthStatus,
    pub runtime_mode: String,
    pub execution_mode: GatewayExecutionMode,
    pub v2_compatibility: GatewayV2CompatibilityMode,
    pub v2_transport: GatewayV2TransportConfig,
    pub v2_connections: GatewayV2ConnectionHealth,
    pub pending_server_request_count: usize,
    pub pending_server_request_kind_counts: BTreeMap<String, usize>,
    pub pending_server_request_route_counts: Vec<GatewayPendingServerRequestRouteCounts>,
    pub pending_server_request_oldest_at: Option<i64>,
    pub remote_workers: Option<Vec<GatewayRemoteWorkerHealth>>,
    pub remote_account_labels_complete: Option<bool>,
    pub remote_unlabeled_account_worker_count: Option<usize>,
    pub remote_unlabeled_account_worker_ids: Option<Vec<usize>>,
    pub remote_unlabeled_account_workers: Option<Vec<GatewayRemoteUnlabeledAccountWorker>>,
    pub project_worker_routes: Option<Vec<GatewayProjectWorkerRoute>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRemoteUnlabeledAccountWorker {
    pub worker_id: usize,
    pub websocket_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPendingServerRequestRouteCounts {
    pub worker_id: Option<usize>,
    pub count: usize,
    pub kind_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadRequest {
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub ephemeral: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayThread {
    pub id: String,
    pub preview: String,
    pub ephemeral: bool,
    pub model_provider: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: GatewayThreadStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GatewayThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    #[serde(rename_all = "camelCase")]
    Active {
        active_flags: Vec<GatewayThreadActiveFlag>,
    },
}

impl From<ThreadStatus> for GatewayThreadStatus {
    fn from(value: ThreadStatus) -> Self {
        match value {
            ThreadStatus::NotLoaded => Self::NotLoaded,
            ThreadStatus::Idle => Self::Idle,
            ThreadStatus::SystemError => Self::SystemError,
            ThreadStatus::Active { active_flags } => Self::Active {
                active_flags: active_flags.into_iter().map(Into::into).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

impl From<ThreadActiveFlag> for GatewayThreadActiveFlag {
    fn from(value: ThreadActiveFlag) -> Self {
        match value {
            ThreadActiveFlag::WaitingOnApproval => Self::WaitingOnApproval,
            ThreadActiveFlag::WaitingOnUserInput => Self::WaitingOnUserInput,
        }
    }
}

impl From<Thread> for GatewayThread {
    fn from(value: Thread) -> Self {
        Self {
            id: value.id,
            preview: value.preview,
            ephemeral: value.ephemeral,
            model_provider: value.model_provider,
            created_at: value.created_at,
            updated_at: value.updated_at,
            status: value.status.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadResponse {
    pub thread: GatewayThread,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThreadsRequest {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub sort_key: Option<GatewayThreadSortKey>,
    pub sort_direction: Option<GatewaySortDirection>,
    pub archived: Option<bool>,
    pub cwd: Option<String>,
    pub search_term: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayThreadSortKey {
    CreatedAt,
    UpdatedAt,
}

impl From<GatewayThreadSortKey> for ThreadSortKey {
    fn from(value: GatewayThreadSortKey) -> Self {
        match value {
            GatewayThreadSortKey::CreatedAt => Self::CreatedAt,
            GatewayThreadSortKey::UpdatedAt => Self::UpdatedAt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewaySortDirection {
    Asc,
    Desc,
}

impl From<GatewaySortDirection> for SortDirection {
    fn from(value: GatewaySortDirection) -> Self {
        match value {
            GatewaySortDirection::Asc => Self::Asc,
            GatewaySortDirection::Desc => Self::Desc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThreadsResponse {
    pub data: Vec<GatewayThread>,
    pub next_cursor: Option<String>,
    pub backwards_cursor: Option<String>,
}

impl From<AppServerThreadListResponse> for ListThreadsResponse {
    fn from(value: AppServerThreadListResponse) -> Self {
        Self {
            data: value.data.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
            backwards_cursor: value.backwards_cursor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTurnRequest {
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayTurn {
    pub id: String,
    pub status: GatewayTurnStatus,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GatewayTurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

impl From<TurnStatus> for GatewayTurnStatus {
    fn from(value: TurnStatus) -> Self {
        match value {
            TurnStatus::Completed => Self::Completed,
            TurnStatus::Interrupted => Self::Interrupted,
            TurnStatus::Failed => Self::Failed,
            TurnStatus::InProgress => Self::InProgress,
        }
    }
}

impl From<Turn> for GatewayTurn {
    fn from(value: Turn) -> Self {
        Self {
            id: value.id,
            status: value.status.into(),
            started_at: value.started_at,
            completed_at: value.completed_at,
            duration_ms: value.duration_ms,
            error_message: value.error.map(|error| error.message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResponse {
    pub turn: GatewayTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptTurnResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayServerRequestEnvelope {
    pub request_id: RequestId,
    pub request: GatewayServerRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GatewayServerRequest {
    #[serde(rename_all = "camelCase")]
    CommandExecutionApproval {
        thread_id: String,
        turn_id: String,
        item_id: String,
        started_at_ms: i64,
        approval_id: Option<String>,
        reason: Option<String>,
        network_approval_context: Option<NetworkApprovalContext>,
        command: Option<String>,
        cwd: Option<String>,
        command_actions: Option<Vec<CommandAction>>,
        additional_permissions: Option<AdditionalPermissionProfile>,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
        proposed_network_policy_amendments: Option<Vec<NetworkPolicyAmendment>>,
        available_decisions: Option<Vec<CommandExecutionApprovalDecision>>,
    },
    #[serde(rename_all = "camelCase")]
    FileChangeApproval {
        thread_id: String,
        turn_id: String,
        item_id: String,
        started_at_ms: i64,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    },
    #[serde(rename_all = "camelCase")]
    PermissionsApproval {
        thread_id: String,
        turn_id: String,
        item_id: String,
        environment_id: Option<String>,
        started_at_ms: i64,
        cwd: PathBuf,
        reason: Option<String>,
        permissions: RequestPermissionProfile,
    },
    #[serde(rename_all = "camelCase")]
    ToolRequestUserInput {
        thread_id: String,
        turn_id: String,
        item_id: String,
        questions: Vec<ToolRequestUserInputQuestion>,
        auto_resolution_ms: Option<u64>,
    },
}

impl GatewayServerRequest {
    pub fn thread_id(&self) -> &str {
        match self {
            Self::CommandExecutionApproval { thread_id, .. }
            | Self::FileChangeApproval { thread_id, .. }
            | Self::PermissionsApproval { thread_id, .. }
            | Self::ToolRequestUserInput { thread_id, .. } => thread_id,
        }
    }

    pub fn kind(&self) -> GatewayServerRequestKind {
        match self {
            Self::CommandExecutionApproval { .. } => {
                GatewayServerRequestKind::CommandExecutionApproval
            }
            Self::FileChangeApproval { .. } => GatewayServerRequestKind::FileChangeApproval,
            Self::PermissionsApproval { .. } => GatewayServerRequestKind::PermissionsApproval,
            Self::ToolRequestUserInput { .. } => GatewayServerRequestKind::ToolRequestUserInput,
        }
    }
}

impl TryFrom<ServerRequest> for GatewayServerRequest {
    type Error = ServerRequest;

    fn try_from(value: ServerRequest) -> Result<Self, Self::Error> {
        match value {
            ServerRequest::CommandExecutionRequestApproval { params, .. } => {
                let CommandExecutionRequestApprovalParams {
                    thread_id,
                    turn_id,
                    item_id,
                    started_at_ms,
                    approval_id,
                    reason,
                    network_approval_context,
                    command,
                    cwd,
                    command_actions,
                    additional_permissions,
                    proposed_execpolicy_amendment,
                    proposed_network_policy_amendments,
                    available_decisions,
                    ..
                } = params;
                Ok(Self::CommandExecutionApproval {
                    thread_id,
                    turn_id,
                    item_id,
                    started_at_ms,
                    approval_id,
                    reason,
                    network_approval_context,
                    command,
                    cwd: cwd.map(|cwd| cwd.as_path().display().to_string()),
                    command_actions,
                    additional_permissions,
                    proposed_execpolicy_amendment,
                    proposed_network_policy_amendments,
                    available_decisions,
                })
            }
            ServerRequest::FileChangeRequestApproval { params, .. } => {
                let FileChangeRequestApprovalParams {
                    thread_id,
                    turn_id,
                    item_id,
                    started_at_ms,
                    reason,
                    grant_root,
                    ..
                } = params;
                Ok(Self::FileChangeApproval {
                    thread_id,
                    turn_id,
                    item_id,
                    started_at_ms,
                    reason,
                    grant_root,
                })
            }
            ServerRequest::PermissionsRequestApproval { params, .. } => {
                let PermissionsRequestApprovalParams {
                    thread_id,
                    turn_id,
                    item_id,
                    environment_id,
                    started_at_ms,
                    cwd,
                    reason,
                    permissions,
                    ..
                } = params;
                Ok(Self::PermissionsApproval {
                    thread_id,
                    turn_id,
                    item_id,
                    environment_id,
                    started_at_ms,
                    cwd: cwd.to_path_buf(),
                    reason,
                    permissions,
                })
            }
            ServerRequest::ToolRequestUserInput { params, .. } => {
                let ToolRequestUserInputParams {
                    thread_id,
                    turn_id,
                    item_id,
                    questions,
                    auto_resolution_ms,
                    ..
                } = params;
                Ok(Self::ToolRequestUserInput {
                    thread_id,
                    turn_id,
                    item_id,
                    questions,
                    auto_resolution_ms,
                })
            }
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayServerRequestKind {
    CommandExecutionApproval,
    FileChangeApproval,
    PermissionsApproval,
    ToolRequestUserInput,
}

impl GatewayServerRequestKind {
    pub(crate) fn metric_tag(self) -> &'static str {
        match self {
            Self::CommandExecutionApproval => "commandExecutionApproval",
            Self::FileChangeApproval => "fileChangeApproval",
            Self::PermissionsApproval => "permissionsApproval",
            Self::ToolRequestUserInput => "toolRequestUserInput",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveServerRequestRequest {
    pub request_id: RequestId,
    #[serde(flatten)]
    pub response: GatewayServerRequestResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GatewayServerRequestResponse {
    #[serde(rename_all = "camelCase")]
    CommandExecutionApproval {
        decision: CommandExecutionApprovalDecision,
    },
    #[serde(rename_all = "camelCase")]
    FileChangeApproval {
        decision: FileChangeApprovalDecision,
    },
    #[serde(rename_all = "camelCase")]
    PermissionsApproval {
        permissions: GrantedPermissionProfile,
        scope: PermissionGrantScope,
    },
    #[serde(rename_all = "camelCase")]
    ToolRequestUserInput {
        answers: HashMap<String, ToolRequestUserInputAnswer>,
    },
}

impl GatewayServerRequestResponse {
    pub fn kind(&self) -> GatewayServerRequestKind {
        match self {
            Self::CommandExecutionApproval { .. } => {
                GatewayServerRequestKind::CommandExecutionApproval
            }
            Self::FileChangeApproval { .. } => GatewayServerRequestKind::FileChangeApproval,
            Self::PermissionsApproval { .. } => GatewayServerRequestKind::PermissionsApproval,
            Self::ToolRequestUserInput { .. } => GatewayServerRequestKind::ToolRequestUserInput,
        }
    }

    pub fn into_result(self) -> serde_json::Result<serde_json::Value> {
        match self {
            Self::CommandExecutionApproval { decision } => {
                serde_json::to_value(CommandExecutionRequestApprovalResponse { decision })
            }
            Self::FileChangeApproval { decision } => {
                serde_json::to_value(FileChangeRequestApprovalResponse { decision })
            }
            Self::PermissionsApproval { permissions, scope } => {
                serde_json::to_value(PermissionsRequestApprovalResponse {
                    permissions,
                    scope,
                    strict_auto_review: None,
                })
            }
            Self::ToolRequestUserInput { answers } => {
                serde_json::to_value(ToolRequestUserInputResponse { answers })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveServerRequestResponse {
    pub status: &'static str,
}
