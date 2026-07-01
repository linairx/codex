use super::map_event_to_sse;
use super::router;
use super::router_with_observability;
use crate::admission::GatewayAdmissionConfig;
use crate::admission::GatewayAdmissionController;
use crate::api::CreateThreadRequest;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthResponse;
use crate::api::GatewayHealthStatus;
use crate::api::GatewayRemoteUnlabeledAccountWorker;
use crate::api::GatewayRemoteWorkerHealth;
use crate::api::GatewayThread;
use crate::api::GatewayThreadActiveFlag;
use crate::api::GatewayThreadStatus;
use crate::api::GatewayTurn;
use crate::api::GatewayTurnStatus;
use crate::api::GatewayV2CompatibilityMode;
use crate::api::GatewayV2ConnectionHealth;
use crate::api::GatewayV2TransportConfig;
use crate::api::InterruptTurnResponse;
use crate::api::ListThreadsRequest;
use crate::api::ListThreadsResponse;
use crate::api::ResolveServerRequestRequest;
use crate::api::ResolveServerRequestResponse;
use crate::api::StartTurnRequest;
use crate::api::ThreadResponse;
use crate::api::TurnResponse;
use crate::auth::GatewayAuth;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use crate::northbound::v2::GatewayV2Timeouts;
use crate::observability::GatewayObservability;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use async_trait::async_trait;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use codex_app_server_protocol::Account;
use codex_app_server_protocol::CancelLoginAccountResponse;
use codex_app_server_protocol::CancelLoginAccountStatus;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::LogoutAccountResponse;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::broadcast;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tower::ServiceExt;

struct FakeRuntime {
    events: broadcast::Sender<GatewayEvent>,
    start_turn_error: Option<String>,
    interrupt_turn_error: Option<String>,
    resolve_server_request_error: Option<String>,
    health_response: Option<GatewayHealthResponse>,
    openai_login_callback_ports: Arc<Mutex<Vec<Option<u16>>>>,
}

impl Default for FakeRuntime {
    fn default() -> Self {
        let (events, _rx) = broadcast::channel(16);
        Self {
            events,
            start_turn_error: None,
            interrupt_turn_error: None,
            resolve_server_request_error: None,
            health_response: None,
            openai_login_callback_ports: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

fn default_v2_connections() -> GatewayV2ConnectionHealth {
    GatewayV2ConnectionHealth::default()
}

#[async_trait]
impl GatewayRuntime for FakeRuntime {
    async fn read_openai_account(
        &self,
        refresh_token: bool,
    ) -> Result<GetAccountResponse, GatewayError> {
        Ok(GetAccountResponse {
            account: Some(Account::ApiKey {}),
            requires_openai_auth: refresh_token,
        })
    }

    async fn login_openai_api_key(
        &self,
        _api_key: String,
    ) -> Result<LoginAccountResponse, GatewayError> {
        Ok(LoginAccountResponse::ApiKey {})
    }

    async fn start_openai_chatgpt_login(
        &self,
        callback_port: Option<u16>,
    ) -> Result<LoginAccountResponse, GatewayError> {
        self.openai_login_callback_ports
            .lock()
            .expect("callback port lock should not be poisoned")
            .push(callback_port);
        Ok(LoginAccountResponse::Chatgpt {
            login_id: "login-1".to_string(),
            auth_url: "https://chatgpt.com/auth".to_string(),
        })
    }

    async fn cancel_openai_login(
        &self,
        _login_id: String,
    ) -> Result<CancelLoginAccountResponse, GatewayError> {
        Ok(CancelLoginAccountResponse {
            status: CancelLoginAccountStatus::Canceled,
        })
    }

    async fn logout_openai_account(&self) -> Result<LogoutAccountResponse, GatewayError> {
        Ok(LogoutAccountResponse {})
    }

    async fn create_thread(
        &self,
        _context: GatewayRequestContext,
        request: CreateThreadRequest,
    ) -> Result<ThreadResponse, GatewayError> {
        Ok(ThreadResponse {
            thread: GatewayThread {
                id: "thread-1".to_string(),
                preview: request.cwd.unwrap_or_default(),
                ephemeral: false,
                model_provider: "openai".to_string(),
                created_at: 1,
                updated_at: 1,
                status: GatewayThreadStatus::Active {
                    active_flags: vec![GatewayThreadActiveFlag::WaitingOnUserInput],
                },
            },
        })
    }

    async fn list_threads(
        &self,
        _context: GatewayRequestContext,
        request: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, GatewayError> {
        Ok(ListThreadsResponse {
            data: vec![GatewayThread {
                id: "thread-1".to_string(),
                preview: request.search_term.unwrap_or_else(|| "preview".to_string()),
                ephemeral: false,
                model_provider: "openai".to_string(),
                created_at: 1,
                updated_at: 2,
                status: GatewayThreadStatus::Idle,
            }],
            next_cursor: Some("cursor-2".to_string()),
            backwards_cursor: Some("cursor-0".to_string()),
        })
    }

    async fn read_thread(
        &self,
        _context: GatewayRequestContext,
        thread_id: String,
    ) -> Result<ThreadResponse, GatewayError> {
        Ok(ThreadResponse {
            thread: GatewayThread {
                id: thread_id,
                preview: "preview".to_string(),
                ephemeral: false,
                model_provider: "openai".to_string(),
                created_at: 1,
                updated_at: 1,
                status: GatewayThreadStatus::Idle,
            },
        })
    }

    async fn start_turn(
        &self,
        _context: GatewayRequestContext,
        _thread_id: String,
        request: StartTurnRequest,
    ) -> Result<TurnResponse, GatewayError> {
        if let Some(error) = &self.start_turn_error {
            return Err(GatewayError::Upstream(error.clone()));
        }
        Ok(TurnResponse {
            turn: GatewayTurn {
                id: "turn-1".to_string(),
                status: GatewayTurnStatus::InProgress,
                started_at: Some(1),
                completed_at: None,
                duration_ms: None,
                error_message: Some(request.input),
            },
        })
    }

    async fn interrupt_turn(
        &self,
        _context: GatewayRequestContext,
        _thread_id: String,
        _turn_id: String,
    ) -> Result<InterruptTurnResponse, GatewayError> {
        if let Some(error) = &self.interrupt_turn_error {
            return Err(GatewayError::Upstream(error.clone()));
        }
        Ok(InterruptTurnResponse { status: "accepted" })
    }

    async fn resolve_server_request(
        &self,
        _context: GatewayRequestContext,
        _request: ResolveServerRequestRequest,
    ) -> Result<ResolveServerRequestResponse, GatewayError> {
        if let Some(error) = &self.resolve_server_request_error {
            return Err(GatewayError::Upstream(error.clone()));
        }
        Ok(ResolveServerRequestResponse { status: "accepted" })
    }

    fn health(&self) -> GatewayHealthResponse {
        if let Some(health_response) = &self.health_response {
            return health_response.clone();
        }

        GatewayHealthResponse {
            status: GatewayHealthStatus::Ok,
            runtime_mode: "embedded".to_string(),
            execution_mode: GatewayExecutionMode::InProcess,
            v2_compatibility: GatewayV2CompatibilityMode::Embedded,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connections: default_v2_connections(),
            pending_server_request_count: 0,
            pending_server_request_kind_counts: std::collections::BTreeMap::new(),
            pending_server_request_route_counts: Vec::new(),
            pending_server_request_oldest_at: None,
            remote_workers: None,
            remote_account_labels_complete: None,
            remote_unlabeled_account_worker_count: None,
            remote_unlabeled_account_worker_ids: None,
            remote_unlabeled_account_workers: None,
            project_worker_routes: None,
            worker_pool: None,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    fn event_visible_to(&self, _context: &GatewayRequestContext, _event: &GatewayEvent) -> bool {
        true
    }
}

#[path = "http_tests_health_basic.rs"]
mod http_tests_health_basic;

#[path = "http_tests_health_project_routes.rs"]
mod http_tests_health_project_routes;

#[path = "http_tests_health_project_routes_selection.rs"]
mod http_tests_health_project_routes_selection;

#[path = "http_tests_health_remote.rs"]
mod http_tests_health_remote;

#[path = "http_tests_health_v2_connections.rs"]
mod http_tests_health_v2_connections;

#[path = "http_tests_v1_routes.rs"]
mod http_tests_v1_routes;

#[path = "http_tests_sse.rs"]
mod http_tests_sse;

#[path = "http_tests_auth_errors.rs"]
mod http_tests_auth_errors;

#[path = "http_tests_auth_openai.rs"]
mod http_tests_auth_openai;

#[path = "http_tests_metrics_admission.rs"]
mod http_tests_metrics_admission;
