use super::DownstreamServerRequestKey;
use super::DownstreamWorkerEvent;
use super::DownstreamWorkerHandle;
use super::GatewayV2ConnectionContext;
use super::GatewayV2DownstreamRouter;
use super::GatewayV2EventState;
use super::GatewayV2State;
use super::GatewayV2Timeouts;
use super::PendingServerRequestRoute;
use super::ResolvedServerRequestRoute;
use super::await_io_with_timeout;
use super::handle_app_server_event;
use super::tagged_type_to_notification;
use super::websocket_upgrade_handler;
use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::auth::GatewayAuth;
use crate::northbound::v2_connection_runtime::log_degraded_multi_worker_thread_discovery;
use crate::northbound::v2_pagination::aggregated_page_bounds;
use crate::northbound::v2_scope::DeduplicatedThreadListEntryLog;
use crate::northbound::v2_scope::log_deduplicated_thread_list_entry;
use crate::northbound::v2_server_requests::log_dropped_duplicate_resolved_server_request;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use crate::v2::gateway_initialize_response;
use axum::Router;
use axum::extract::WebSocketUpgrade;
use axum::extract::ws::close_code;
use axum::http::StatusCode;
use axum::routing::any;
use codex_app_server_client::AppServerEvent;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_client::EnvironmentManager;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_client::InProcessClientStartArgs;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_protocol::ApplyPatchApprovalResponse;
use codex_app_server_protocol::AppsListResponse;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::CollaborationModeListResponse;
use codex_app_server_protocol::CommandExecOutputDeltaNotification;
use codex_app_server_protocol::CommandExecOutputStream;
use codex_app_server_protocol::CommandExecutionOutputDeltaNotification;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::ContextCompactedNotification;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::ExecCommandApprovalResponse;
use codex_app_server_protocol::ExperimentalFeatureListResponse;
use codex_app_server_protocol::FileChangeOutputDeltaNotification;
use codex_app_server_protocol::FsUnwatchResponse;
use codex_app_server_protocol::FsWatchParams;
use codex_app_server_protocol::FsWatchResponse;
use codex_app_server_protocol::FuzzyFileSearchMatchType;
use codex_app_server_protocol::FuzzyFileSearchResponse;
use codex_app_server_protocol::FuzzyFileSearchResult;
use codex_app_server_protocol::FuzzyFileSearchSessionCompletedNotification;
use codex_app_server_protocol::FuzzyFileSearchSessionUpdatedNotification;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::HookCompletedNotification;
use codex_app_server_protocol::HookStartedNotification;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::ItemGuardianApprovalReviewCompletedNotification;
use codex_app_server_protocol::ItemGuardianApprovalReviewStartedNotification;
use codex_app_server_protocol::ItemStartedNotification;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::ListMcpServerStatusResponse;
use codex_app_server_protocol::McpToolCallProgressNotification;
use codex_app_server_protocol::ModelRerouteReason;
use codex_app_server_protocol::ModelReroutedNotification;
use codex_app_server_protocol::PlanDeltaNotification;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginSummary;
use codex_app_server_protocol::RawResponseItemCompletedNotification;
use codex_app_server_protocol::ReasoningSummaryPartAddedNotification;
use codex_app_server_protocol::ReasoningSummaryTextDeltaNotification;
use codex_app_server_protocol::ReasoningTextDeltaNotification;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ServerRequestResolvedNotification;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::TerminalInteractionNotification;
use codex_app_server_protocol::ThreadArchivedNotification;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadNameUpdatedNotification;
use codex_app_server_protocol::ThreadRealtimeListVoicesResponse;
use codex_app_server_protocol::ThreadTokenUsage;
use codex_app_server_protocol::ThreadTokenUsageUpdatedNotification;
use codex_app_server_protocol::ThreadUnarchivedNotification;
use codex_app_server_protocol::TokenUsageBreakdown;
use codex_app_server_protocol::TurnDiffUpdatedNotification;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::TurnPlanStep;
use codex_app_server_protocol::TurnPlanStepStatus;
use codex_app_server_protocol::TurnPlanUpdatedNotification;
use codex_app_server_protocol::WarningNotification;
use codex_arg0::Arg0DispatchPaths;
use codex_config::CloudConfigBundleLoader;
use codex_core::config::Config;
use codex_core::config::LoaderOverrides;
use codex_feedback::CodexFeedback;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::RealtimeVoice;
use codex_protocol::protocol::RealtimeVoicesList;
use codex_protocol::protocol::ReviewDecision;
use codex_protocol::protocol::SessionSource;
use futures::SinkExt;
use futures::StreamExt;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use serde_json::Value;
use socket2::Domain;
use socket2::Protocol;
use socket2::Socket;
use socket2::Type;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Instant;
use tempfile::tempdir;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Error as WebSocketError;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::Request as WebSocketRequest;
use tokio_tungstenite::tungstenite::handshake::server::Response as WebSocketResponse;
use tracing_subscriber::layer::SubscriberExt;

static ASYNC_LOG_CAPTURE_LOCK: std::sync::LazyLock<Semaphore> =
    std::sync::LazyLock::new(|| Semaphore::new(1));
static SYNC_LOG_CAPTURE_LOCK: std::sync::LazyLock<StdMutex<()>> =
    std::sync::LazyLock::new(|| StdMutex::new(()));

fn test_worker_websocket_url(worker_id: Option<usize>) -> String {
    match worker_id {
        Some(worker_id @ 0..=25) => {
            let worker_letter = (b'a' + worker_id as u8) as char;
            format!("ws://worker-{worker_letter}.invalid")
        }
        Some(worker_id) => format!("ws://worker-{worker_id}.invalid"),
        None => "<embedded>".to_string(),
    }
}

async fn connect_websocket_with_small_receive_buffer(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let domain = match addr {
        SocketAddr::V4(_) => Domain::IPV4,
        SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .expect("slow-client socket should create");
    socket
        .set_recv_buffer_size(4096)
        .expect("slow-client receive buffer should configure");
    socket
        .connect(&addr.into())
        .expect("slow-client socket should connect");
    socket
        .set_nonblocking(true)
        .expect("slow-client socket should become nonblocking");
    let stream = tokio::net::TcpStream::from_std(socket.into())
        .expect("slow-client stream should convert to tokio");
    let (websocket, _response) = tokio_tungstenite::client_async(
        format!("ws://{addr}/"),
        tokio_tungstenite::MaybeTlsStream::Plain(stream),
    )
    .await
    .expect("slow-client websocket should connect");
    websocket
}

#[cfg(test)]
#[path = "v2_tests_cases.rs"]
mod cases;
