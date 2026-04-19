use crate::adapter::thread_list_request;
use crate::adapter::thread_read_request;
use crate::adapter::thread_start_request;
use crate::adapter::turn_interrupt_request;
use crate::adapter::turn_start_request;
use crate::api::CreateThreadRequest;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthResponse;
use crate::api::GatewayHealthStatus;
use crate::api::GatewayServerRequest;
use crate::api::GatewayV2CompatibilityMode;
use crate::api::GatewayV2TransportConfig;
use crate::api::InterruptTurnResponse;
use crate::api::ListThreadsRequest;
use crate::api::ListThreadsResponse;
use crate::api::ResolveServerRequestRequest;
use crate::api::ResolveServerRequestResponse;
use crate::api::StartTurnRequest;
use crate::api::ThreadResponse;
use crate::api::TurnResponse;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use async_trait::async_trait;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadListResponse as AppServerThreadListResponse;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnInterruptResponse as AppServerTurnInterruptResponse;
use codex_app_server_protocol::TurnStartResponse;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;

#[async_trait]
/// Runtime interface that backs the gateway's northbound HTTP surface.
///
/// Implementations are responsible for mapping gateway operations onto the
/// underlying execution stack, such as an embedded or remote app-server.
pub trait GatewayRuntime: Send + Sync {
    async fn create_thread(
        &self,
        context: GatewayRequestContext,
        request: CreateThreadRequest,
    ) -> Result<ThreadResponse, GatewayError>;
    async fn list_threads(
        &self,
        context: GatewayRequestContext,
        request: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, GatewayError>;
    async fn read_thread(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
    ) -> Result<ThreadResponse, GatewayError>;
    async fn start_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        request: StartTurnRequest,
    ) -> Result<TurnResponse, GatewayError>;
    async fn interrupt_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        turn_id: String,
    ) -> Result<InterruptTurnResponse, GatewayError>;
    async fn resolve_server_request(
        &self,
        context: GatewayRequestContext,
        request: ResolveServerRequestRequest,
    ) -> Result<ResolveServerRequestResponse, GatewayError>;
    fn health(&self) -> GatewayHealthResponse;
    fn subscribe(&self) -> broadcast::Receiver<GatewayEvent>;
    fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool;
}

#[derive(Clone)]
pub struct AppServerGatewayRuntime {
    app_server: AppServerRequestHandle,
    worker_id: Option<usize>,
    execution_mode: GatewayExecutionMode,
    next_request_id: Arc<AtomicI64>,
    events: broadcast::Sender<GatewayEvent>,
    scope_registry: Arc<GatewayScopeRegistry>,
    remote_worker_health: Option<Arc<RemoteWorkerHealthRegistry>>,
    v2_transport: GatewayV2TransportConfig,
}

impl AppServerGatewayRuntime {
    pub fn new(
        app_server: AppServerRequestHandle,
        execution_mode: GatewayExecutionMode,
        events: broadcast::Sender<GatewayEvent>,
        scope_registry: Arc<GatewayScopeRegistry>,
        v2_transport: GatewayV2TransportConfig,
    ) -> Self {
        Self::new_with_worker_id(
            app_server,
            None,
            execution_mode,
            events,
            scope_registry,
            None,
            v2_transport,
        )
    }

    pub fn new_with_worker_id(
        app_server: AppServerRequestHandle,
        worker_id: Option<usize>,
        execution_mode: GatewayExecutionMode,
        events: broadcast::Sender<GatewayEvent>,
        scope_registry: Arc<GatewayScopeRegistry>,
        remote_worker_health: Option<Arc<RemoteWorkerHealthRegistry>>,
        v2_transport: GatewayV2TransportConfig,
    ) -> Self {
        Self {
            app_server,
            worker_id,
            execution_mode,
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry,
            remote_worker_health,
            v2_transport,
        }
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::Integer(self.next_request_id.fetch_add(1, Ordering::Relaxed))
    }

    pub fn publish_event(&self, event: GatewayEvent) {
        let _ = self.events.send(event);
    }

    pub fn thread_context(&self, thread_id: &str) -> Option<GatewayRequestContext> {
        self.scope_registry.thread_context(thread_id)
    }

    pub fn store_pending_server_request(
        &self,
        request_id: RequestId,
        request: &GatewayServerRequest,
        context: GatewayRequestContext,
    ) {
        self.scope_registry
            .register_pending_server_request_with_worker(
                request_id,
                request.kind(),
                context,
                self.worker_id,
            );
    }

    pub fn clear_pending_server_request(&self, request_id: &RequestId) {
        self.scope_registry.clear_pending_server_request(request_id);
    }

    pub fn clear_all_pending_server_requests(&self) {
        self.scope_registry.clear_all_pending_server_requests();
    }

    pub fn clear_owned_pending_server_requests(&self) {
        if let Some(worker_id) = self.worker_id {
            self.scope_registry
                .clear_pending_server_requests_for_worker(worker_id);
        } else {
            self.scope_registry.clear_all_pending_server_requests();
        }
    }

    pub fn mark_worker_unhealthy(&self, error: Option<String>) {
        if let Some(remote_worker_health) = &self.remote_worker_health
            && let Some(worker_id) = self.worker_id
        {
            remote_worker_health.mark_unhealthy(worker_id, error);
        }
    }

    pub fn mark_worker_healthy(&self) {
        if let Some(remote_worker_health) = &self.remote_worker_health
            && let Some(worker_id) = self.worker_id
        {
            remote_worker_health.mark_healthy(worker_id);
        }
    }

    fn ensure_thread_access(
        &self,
        context: &GatewayRequestContext,
        thread_id: &str,
    ) -> Result<(), GatewayError> {
        if self.scope_registry.thread_visible_to(context, thread_id) {
            Ok(())
        } else {
            Err(GatewayError::NotFound(format!(
                "thread not found: {thread_id}"
            )))
        }
    }
}

#[async_trait]
impl GatewayRuntime for AppServerGatewayRuntime {
    async fn create_thread(
        &self,
        context: GatewayRequestContext,
        request: CreateThreadRequest,
    ) -> Result<ThreadResponse, GatewayError> {
        let response: ThreadStartResponse = self
            .app_server
            .request_typed(thread_start_request(self.next_request_id(), request))
            .await?;
        self.scope_registry.register_thread_with_worker(
            response.thread.id.clone(),
            context,
            self.worker_id,
        );

        Ok(ThreadResponse {
            thread: response.thread.into(),
        })
    }

    async fn list_threads(
        &self,
        context: GatewayRequestContext,
        request: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, GatewayError> {
        let mut response: AppServerThreadListResponse = self
            .app_server
            .request_typed(thread_list_request(self.next_request_id(), request))
            .await?;
        let visible_thread_ids = self.scope_registry.filter_thread_ids(
            &context,
            response.data.iter().map(|thread| thread.id.clone()),
        );
        let visible_thread_ids = visible_thread_ids.into_iter().collect::<HashSet<_>>();
        response
            .data
            .retain(|thread| visible_thread_ids.contains(&thread.id));

        Ok(response.into())
    }

    async fn read_thread(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
    ) -> Result<ThreadResponse, GatewayError> {
        self.ensure_thread_access(&context, &thread_id)?;
        let response: ThreadReadResponse = self
            .app_server
            .request_typed(thread_read_request(self.next_request_id(), thread_id))
            .await?;

        Ok(ThreadResponse {
            thread: response.thread.into(),
        })
    }

    async fn start_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        request: StartTurnRequest,
    ) -> Result<TurnResponse, GatewayError> {
        self.ensure_thread_access(&context, &thread_id)?;
        if request.input.trim().is_empty() {
            return Err(GatewayError::InvalidRequest(
                "turn input must not be empty".to_string(),
            ));
        }

        let response: TurnStartResponse = self
            .app_server
            .request_typed(turn_start_request(
                self.next_request_id(),
                thread_id,
                request,
            ))
            .await?;

        Ok(TurnResponse {
            turn: response.turn.into(),
        })
    }

    async fn interrupt_turn(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
        turn_id: String,
    ) -> Result<InterruptTurnResponse, GatewayError> {
        self.ensure_thread_access(&context, &thread_id)?;
        let _: AppServerTurnInterruptResponse = self
            .app_server
            .request_typed(turn_interrupt_request(
                self.next_request_id(),
                thread_id,
                turn_id,
            ))
            .await?;

        Ok(InterruptTurnResponse { status: "accepted" })
    }

    async fn resolve_server_request(
        &self,
        context: GatewayRequestContext,
        request: ResolveServerRequestRequest,
    ) -> Result<ResolveServerRequestResponse, GatewayError> {
        let Some(pending_request) = self
            .scope_registry
            .take_pending_server_request(&request.request_id)
        else {
            return Err(GatewayError::NotFound(format!(
                "server request not found: {}",
                request.request_id
            )));
        };

        if pending_request.context != context {
            return Err(GatewayError::NotFound(format!(
                "server request not found: {}",
                request.request_id
            )));
        }

        if request.response.kind() != pending_request.kind {
            return Err(GatewayError::InvalidRequest(
                "server request response type does not match pending request".to_string(),
            ));
        }

        let result = request.response.into_result().map_err(|err| {
            GatewayError::InvalidRequest(format!("invalid server request response payload: {err}"))
        })?;
        self.app_server
            .resolve_server_request(request.request_id, result)
            .await
            .map_err(|err| {
                GatewayError::Upstream(format!("server request resolve failed: {err}"))
            })?;

        Ok(ResolveServerRequestResponse { status: "accepted" })
    }

    fn health(&self) -> GatewayHealthResponse {
        GatewayHealthResponse {
            status: GatewayHealthStatus::Ok,
            runtime_mode: "embedded".to_string(),
            execution_mode: self.execution_mode,
            v2_compatibility: GatewayV2CompatibilityMode::Embedded,
            v2_transport: self.v2_transport,
            remote_workers: None,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool {
        self.scope_registry.event_visible_to(context, event)
    }
}
