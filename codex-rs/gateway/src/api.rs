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
    pub healthy: bool,
    pub reconnecting: bool,
    pub reconnect_attempt_count: u32,
    pub last_error: Option<String>,
    pub last_state_change_at: Option<i64>,
    pub last_error_at: Option<i64>,
    pub next_reconnect_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2TransportConfig {
    pub initialize_timeout_seconds: u64,
    pub client_send_timeout_seconds: u64,
    pub reconnect_retry_backoff_seconds: u64,
    pub max_pending_server_requests: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayV2ConnectionHealth {
    pub active_connection_count: usize,
    pub peak_active_connection_count: usize,
    pub total_connection_count: u64,
    pub last_connection_started_at: Option<i64>,
    pub last_connection_completed_at: Option<i64>,
    pub last_connection_duration_ms: Option<u64>,
    pub last_connection_outcome: Option<String>,
    pub last_connection_detail: Option<String>,
    pub last_connection_pending_server_request_count: usize,
    pub last_connection_answered_but_unresolved_server_request_count: usize,
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
    pub remote_workers: Option<Vec<GatewayRemoteWorkerHealth>>,
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
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    },
    #[serde(rename_all = "camelCase")]
    PermissionsApproval {
        thread_id: String,
        turn_id: String,
        item_id: String,
        reason: Option<String>,
        permissions: RequestPermissionProfile,
    },
    #[serde(rename_all = "camelCase")]
    ToolRequestUserInput {
        thread_id: String,
        turn_id: String,
        item_id: String,
        questions: Vec<ToolRequestUserInputQuestion>,
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
                } = params;
                Ok(Self::CommandExecutionApproval {
                    thread_id,
                    turn_id,
                    item_id,
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
                    reason,
                    grant_root,
                } = params;
                Ok(Self::FileChangeApproval {
                    thread_id,
                    turn_id,
                    item_id,
                    reason,
                    grant_root,
                })
            }
            ServerRequest::PermissionsRequestApproval { params, .. } => {
                let PermissionsRequestApprovalParams {
                    thread_id,
                    turn_id,
                    item_id,
                    reason,
                    permissions,
                } = params;
                Ok(Self::PermissionsApproval {
                    thread_id,
                    turn_id,
                    item_id,
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
                } = params;
                Ok(Self::ToolRequestUserInput {
                    thread_id,
                    turn_id,
                    item_id,
                    questions,
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
                serde_json::to_value(PermissionsRequestApprovalResponse { permissions, scope })
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
