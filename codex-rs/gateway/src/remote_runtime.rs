use crate::adapter::thread_list_request;
use crate::adapter::thread_read_request;
use crate::adapter::thread_start_request;
use crate::adapter::turn_interrupt_request;
use crate::adapter::turn_start_request;
use crate::api::CreateThreadRequest;
use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthResponse;
use crate::api::GatewayHealthStatus;
use crate::api::GatewayRemoteUnlabeledAccountWorker;
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
use crate::config::GatewayRemoteSelectionPolicy;
use crate::error::GatewayError;
use crate::event::GatewayAccountActiveThreadHandoffFailed;
use crate::event::GatewayAccountThreadHandoffFailed;
use crate::event::GatewayAccountThreadHandoffSucceeded;
use crate::event::GatewayEvent;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
use async_trait::async_trait;
use codex_app_server_client::TypedRequestError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadListResponse as AppServerThreadListResponse;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::TurnInterruptResponse as AppServerTurnInterruptResponse;
use codex_app_server_protocol::TurnStartResponse;
use std::cmp::Reverse;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;
use tracing::info;
use tracing::warn;

const DEFAULT_THREAD_LIST_LIMIT: usize = 20;
const GATEWAY_THREAD_CURSOR_PREFIX: &str = "offset:";

pub struct RemoteWorkerGatewayRuntime {
    workers: Vec<GatewayRemoteWorker>,
    selection_policy: GatewayRemoteSelectionPolicy,
    next_worker: AtomicUsize,
    next_request_id: Arc<AtomicI64>,
    events: broadcast::Sender<GatewayEvent>,
    scope_registry: Arc<GatewayScopeRegistry>,
    worker_health: Arc<RemoteWorkerHealthRegistry>,
    v2_transport: GatewayV2TransportConfig,
    v2_connection_health: Arc<GatewayV2ConnectionHealthRegistry>,
    observability: GatewayObservability,
}

impl RemoteWorkerGatewayRuntime {
    pub(crate) fn new(
        workers: Vec<GatewayRemoteWorker>,
        selection_policy: GatewayRemoteSelectionPolicy,
        events: broadcast::Sender<GatewayEvent>,
        scope_registry: Arc<GatewayScopeRegistry>,
        worker_health: Arc<RemoteWorkerHealthRegistry>,
        v2_transport: GatewayV2TransportConfig,
        observability: GatewayObservability,
    ) -> Result<Self, GatewayError> {
        if workers.is_empty() {
            return Err(GatewayError::InvalidRequest(
                "remote gateway runtime requires at least one worker".to_string(),
            ));
        }

        Ok(Self {
            workers: workers.into_iter().collect(),
            selection_policy,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry,
            worker_health,
            v2_transport,
            v2_connection_health: observability.v2_connection_health(),
            observability,
        })
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::Integer(self.next_request_id.fetch_add(1, Ordering::Relaxed))
    }

    fn pick_worker(&self) -> Result<&GatewayRemoteWorker, GatewayError> {
        match self.selection_policy {
            GatewayRemoteSelectionPolicy::RoundRobin => {
                let start = self.next_worker.fetch_add(1, Ordering::Relaxed);
                for offset in 0..self.workers.len() {
                    let index = (start + offset) % self.workers.len();
                    if self.worker_health.is_healthy(index)
                        && self.worker_health.account_has_capacity(index)
                    {
                        return Ok(&self.workers[index]);
                    }
                }

                Err(GatewayError::Upstream(
                    "no healthy remote workers with available account capacity are available"
                        .to_string(),
                ))
            }
        }
    }

    fn project_worker(&self, context: &GatewayRequestContext) -> Option<&GatewayRemoteWorker> {
        let worker_id = self.scope_registry.select_project_worker_id_with_accounts(
            context,
            self.workers
                .iter()
                .filter(|worker| {
                    self.worker_health.is_healthy(worker.id())
                        && self.worker_health.account_has_capacity(worker.id())
                })
                .map(GatewayRemoteWorker::id),
            |worker_id| self.worker_health.account_id(worker_id),
        )?;
        self.workers
            .iter()
            .find(|worker| worker.id() == worker_id)
            .filter(|worker| {
                self.worker_health.is_healthy(worker.id())
                    && self.worker_health.account_has_capacity(worker.id())
            })
    }

    fn pick_worker_for_project(
        &self,
        context: &GatewayRequestContext,
    ) -> Result<&GatewayRemoteWorker, GatewayError> {
        if let Some(worker) = self.project_worker(context) {
            return Ok(worker);
        }
        self.pick_worker()
    }

    fn worker_for_thread(
        &self,
        context: &GatewayRequestContext,
        thread_id: &str,
    ) -> Result<&GatewayRemoteWorker, GatewayError> {
        if !self.scope_registry.thread_visible_to(context, thread_id) {
            return Err(GatewayError::NotFound(format!(
                "thread not found: {thread_id}"
            )));
        }
        let Some(worker_id) = self.scope_registry.thread_worker_id(thread_id) else {
            return Err(GatewayError::Upstream(format!(
                "thread {thread_id} is missing a remote worker route"
            )));
        };
        self.worker(worker_id)
    }

    fn fail_if_active_thread_account_exhausted(
        &self,
        context: &GatewayRequestContext,
        method: &str,
        thread_id: &str,
    ) -> Result<(), GatewayError> {
        if !self.scope_registry.thread_visible_to(context, thread_id) {
            return Ok(());
        }
        let Some(worker_id) = self.scope_registry.thread_worker_id(thread_id) else {
            return Ok(());
        };
        self.fail_if_worker_account_exhausted(context, method, thread_id, worker_id)
    }

    fn fail_if_worker_account_exhausted(
        &self,
        context: &GatewayRequestContext,
        method: &str,
        thread_id: &str,
        worker_id: usize,
    ) -> Result<(), GatewayError> {
        if self.worker_health.account_capacity(worker_id)
            != Some(GatewayAccountCapacityStatus::Exhausted)
        {
            return Ok(());
        }

        let message = format!(
            "thread {thread_id} is pinned to worker {worker_id} with exhausted account capacity for {method}"
        );
        self.observability
            .record_account_capacity_event(worker_id, "active_thread_handoff_failure");
        let _ = self
            .events
            .send(GatewayEvent::account_active_thread_handoff_failed(
                GatewayAccountActiveThreadHandoffFailed {
                    tenant_id: context.tenant_id.as_str(),
                    project_id: context.project_id.as_deref(),
                    method,
                    thread_id,
                    exhausted_worker_id: worker_id,
                    exhausted_account_id: self.worker_health.account_id(worker_id),
                    reason: message.as_str(),
                },
            ));
        warn!(
            method,
            account_capacity_event = "active_thread_handoff_failure",
            tenant_id = context.tenant_id.as_str(),
            project_id = context.project_id.as_deref(),
            thread_id,
            exhausted_worker_id = worker_id,
            exhausted_account_id = self.worker_health.account_id(worker_id),
            "gateway HTTP failed closed for an active thread request pinned to an exhausted account because no protocol-visible context handoff was requested"
        );

        Err(GatewayError::Upstream(message))
    }

    fn mark_account_capacity_exhausted(
        &self,
        context: &GatewayRequestContext,
        method: &str,
        worker_id: usize,
        reason: String,
    ) {
        self.observability
            .record_account_capacity_event(worker_id, "exhausted");
        warn!(
            method,
            tenant_id = context.tenant_id.as_str(),
            project_id = context.project_id.as_deref(),
            exhausted_worker_id = worker_id,
            exhausted_account_id = self.worker_health.account_id(worker_id),
            reason = reason.as_str(),
            "gateway HTTP marked account-backed worker exhausted after downstream request failure"
        );
        let _ = self.events.send(GatewayEvent::account_capacity_exhausted(
            context.tenant_id.as_str(),
            context.project_id.as_deref(),
            worker_id,
            self.worker_health.account_id(worker_id),
            reason.as_str(),
        ));
        self.worker_health
            .mark_account_exhausted_for_worker(worker_id, reason);
    }

    fn record_server_request_delivery_failure(&self, response_kind: &str) {
        self.observability
            .record_server_request_lifecycle_event("client_server_request_answered", response_kind);
        self.record_server_request_delivery_failure_after_answer(response_kind);
    }

    fn record_server_request_delivery_failure_after_answer(&self, response_kind: &str) {
        self.observability.record_server_request_lifecycle_event(
            "client_server_request_delivery_failed",
            response_kind,
        );
        self.observability
            .record_server_request_answer_delivery_failure(response_kind);
    }

    fn worker(&self, worker_id: usize) -> Result<&GatewayRemoteWorker, GatewayError> {
        let worker = self.workers.get(worker_id).ok_or_else(|| {
            GatewayError::Upstream(format!("remote worker route {worker_id} is unavailable"))
        })?;
        if !self.worker_health.is_healthy(worker_id) {
            return Err(GatewayError::Upstream(format!(
                "remote worker route {worker_id} is unavailable because the worker is unhealthy"
            )));
        }

        Ok(worker)
    }

    async fn fetch_all_threads(
        &self,
        worker: &GatewayRemoteWorker,
        request: &ListThreadsRequest,
    ) -> Result<Vec<crate::api::GatewayThread>, GatewayError> {
        let mut all_threads = Vec::new();
        let mut cursor = None;

        loop {
            let response: AppServerThreadListResponse = worker
                .request_handle()
                .request_typed(thread_list_request(
                    self.next_request_id(),
                    ListThreadsRequest {
                        cursor: cursor.clone(),
                        limit: Some(u32::MAX),
                        sort_key: request.sort_key,
                        sort_direction: request.sort_direction,
                        archived: request.archived,
                        cwd: request.cwd.clone(),
                        search_term: request.search_term.clone(),
                    },
                ))
                .await?;
            let next_cursor = response.next_cursor.clone();
            all_threads.extend(response.data.into_iter().map(Into::into));
            if next_cursor.is_none() {
                break;
            }
            cursor = next_cursor;
        }

        Ok(all_threads)
    }

    fn sort_threads(
        &self,
        threads: &mut [crate::api::GatewayThread],
        request: &ListThreadsRequest,
    ) {
        match request
            .sort_key
            .unwrap_or(crate::api::GatewayThreadSortKey::CreatedAt)
        {
            crate::api::GatewayThreadSortKey::CreatedAt => {
                if request
                    .sort_direction
                    .unwrap_or(crate::api::GatewaySortDirection::Desc)
                    == crate::api::GatewaySortDirection::Asc
                {
                    threads.sort_by_key(|thread| (thread.created_at, thread.id.clone()));
                } else {
                    threads.sort_by_key(|thread| Reverse((thread.created_at, thread.id.clone())));
                }
            }
            crate::api::GatewayThreadSortKey::UpdatedAt => {
                if request
                    .sort_direction
                    .unwrap_or(crate::api::GatewaySortDirection::Desc)
                    == crate::api::GatewaySortDirection::Asc
                {
                    threads.sort_by_key(|thread| (thread.updated_at, thread.id.clone()));
                } else {
                    threads.sort_by_key(|thread| Reverse((thread.updated_at, thread.id.clone())));
                }
            }
        }
    }

    fn decode_cursor(cursor: Option<&str>) -> Result<usize, GatewayError> {
        let Some(cursor) = cursor else {
            return Ok(0);
        };
        let Some(offset) = cursor.strip_prefix(GATEWAY_THREAD_CURSOR_PREFIX) else {
            return Err(GatewayError::InvalidRequest(format!(
                "invalid gateway thread cursor: {cursor}"
            )));
        };
        offset.parse::<usize>().map_err(|_| {
            GatewayError::InvalidRequest(format!("invalid gateway thread cursor: {cursor}"))
        })
    }

    fn encode_cursor(offset: usize) -> String {
        format!("{GATEWAY_THREAD_CURSOR_PREFIX}{offset}")
    }
}

#[async_trait]
impl GatewayRuntime for RemoteWorkerGatewayRuntime {
    async fn create_thread(
        &self,
        context: GatewayRequestContext,
        request: CreateThreadRequest,
    ) -> Result<ThreadResponse, GatewayError> {
        let mut exhausted_worker_ids = Vec::new();
        for _ in 0..self.workers.len() {
            let worker = self.pick_worker_for_project(&context)?;
            match worker
                .request_handle()
                .request_typed::<codex_app_server_protocol::ThreadStartResponse>(
                    thread_start_request(self.next_request_id(), request.clone()),
                )
                .await
            {
                Ok(response) => {
                    self.worker_health
                        .mark_account_available_for_worker(worker.id());
                    if !exhausted_worker_ids.is_empty() {
                        let _ = self.events.send(GatewayEvent::account_failover_succeeded(
                            context.tenant_id.as_str(),
                            context.project_id.as_deref(),
                            worker.id(),
                            self.worker_health.account_id(worker.id()),
                            exhausted_worker_ids.clone(),
                        ));
                        self.observability.record_account_capacity_event(
                            worker.id(),
                            "thread_start_failover_success",
                        );
                        info!(
                            method = "thread/start",
                            tenant_id = context.tenant_id.as_str(),
                            project_id = context.project_id.as_deref(),
                            replacement_worker_id = worker.id(),
                            replacement_account_id = self.worker_health.account_id(worker.id()),
                            exhausted_worker_ids = ?exhausted_worker_ids,
                            "gateway HTTP thread/start retried on another account-backed worker after account capacity exhaustion"
                        );
                    }
                    self.scope_registry.register_thread_with_worker(
                        response.thread.id.clone(),
                        context.clone(),
                        Some(worker.id()),
                    );
                    self.scope_registry
                        .register_project_worker(context.clone(), worker.id());
                    if let Some(project_id) = context.project_id.as_deref() {
                        self.observability.record_project_worker_route_selected(
                            worker.id(),
                            context.tenant_id.as_str(),
                            project_id,
                            response.thread.id.as_str(),
                        );
                        let _ = self
                            .events
                            .send(GatewayEvent::project_worker_route_selected(
                                crate::event::GatewayProjectWorkerRouteSelected {
                                    tenant_id: context.tenant_id.as_str(),
                                    project_id,
                                    thread_id: response.thread.id.as_str(),
                                    worker_id: worker.id(),
                                    account_id: self.worker_health.account_id(worker.id()),
                                },
                            ));
                    }

                    return Ok(ThreadResponse {
                        thread: response.thread.into(),
                    });
                }
                Err(error) => {
                    if let Some(reason) = account_capacity_error_reason(&error) {
                        self.mark_account_capacity_exhausted(
                            &context,
                            "thread/start",
                            worker.id(),
                            reason,
                        );
                        exhausted_worker_ids.push(worker.id());
                        continue;
                    }

                    let gateway_error = GatewayError::from(error);
                    if matches!(gateway_error, GatewayError::Upstream(_)) {
                        self.worker_health
                            .mark_unhealthy(worker.id(), Some(format!("{gateway_error:?}")));
                        continue;
                    }

                    return Err(gateway_error);
                }
            }
        }

        Err(GatewayError::Upstream(
            "no healthy remote workers with available account capacity are available".to_string(),
        ))
    }

    async fn list_threads(
        &self,
        context: GatewayRequestContext,
        request: ListThreadsRequest,
    ) -> Result<ListThreadsResponse, GatewayError> {
        let mut data = Vec::new();
        for worker in &self.workers {
            if !self.worker_health.is_healthy(worker.id()) {
                continue;
            }
            match self.fetch_all_threads(worker, &request).await {
                Ok(threads) => data.extend(threads),
                Err(GatewayError::Upstream(_)) => {
                    self.worker_health.mark_unhealthy(
                        worker.id(),
                        Some("thread list failed for remote worker".to_string()),
                    );
                }
                Err(error) => return Err(error),
            }
        }

        data.retain(|thread| self.scope_registry.thread_visible_to(&context, &thread.id));
        self.sort_threads(&mut data, &request);

        let start = Self::decode_cursor(request.cursor.as_deref())?;
        let limit = request
            .limit
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_THREAD_LIST_LIMIT);
        let end = start.saturating_add(limit).min(data.len());
        let page = if start >= data.len() {
            Vec::new()
        } else {
            data[start..end].to_vec()
        };
        let next_cursor = (end < data.len()).then(|| Self::encode_cursor(end));

        Ok(ListThreadsResponse {
            data: page,
            next_cursor,
            backwards_cursor: None,
        })
    }

    async fn read_thread(
        &self,
        context: GatewayRequestContext,
        thread_id: String,
    ) -> Result<ThreadResponse, GatewayError> {
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        if self.worker_health.account_capacity(worker_id)
            == Some(GatewayAccountCapacityStatus::Exhausted)
        {
            let mut first_error = None::<String>;
            for replacement in &self.workers {
                let replacement_worker_id = replacement.id();
                if replacement_worker_id == worker_id
                    || !self.worker_health.is_healthy(replacement_worker_id)
                    || !self
                        .worker_health
                        .account_has_capacity(replacement_worker_id)
                {
                    continue;
                }

                match replacement
                    .request_handle()
                    .request_typed::<ThreadReadResponse>(thread_read_request(
                        self.next_request_id(),
                        thread_id.clone(),
                    ))
                    .await
                {
                    Ok(response) => {
                        if response.thread.id != thread_id {
                            if first_error.is_none() {
                                first_error = Some(format!(
                                    "replacement worker returned thread {} while restoring {thread_id}",
                                    response.thread.id
                                ));
                            }
                            continue;
                        }
                        self.scope_registry.register_thread_with_worker(
                            response.thread.id.clone(),
                            context.clone(),
                            Some(replacement_worker_id),
                        );
                        self.observability.record_account_capacity_event(
                            replacement_worker_id,
                            "thread_read_handoff_success",
                        );
                        let _ = self
                            .events
                            .send(GatewayEvent::account_thread_handoff_succeeded(
                                GatewayAccountThreadHandoffSucceeded {
                                    tenant_id: context.tenant_id.as_str(),
                                    project_id: context.project_id.as_deref(),
                                    method: "thread/read",
                                    thread_id: thread_id.as_str(),
                                    exhausted_worker_id: worker_id,
                                    exhausted_account_id: self.worker_health.account_id(worker_id),
                                    replacement_worker_id,
                                    replacement_account_id: self
                                        .worker_health
                                        .account_id(replacement_worker_id),
                                },
                            ));
                        info!(
                            method = "thread/read",
                            tenant_id = context.tenant_id.as_str(),
                            project_id = context.project_id.as_deref(),
                            thread_id = thread_id.as_str(),
                            exhausted_worker_id = worker_id,
                            exhausted_account_id = self.worker_health.account_id(worker_id),
                            replacement_worker_id,
                            replacement_account_id =
                                self.worker_health.account_id(replacement_worker_id),
                            "gateway HTTP restored a thread/read request on another account-backed worker after account capacity exhaustion"
                        );
                        return Ok(ThreadResponse {
                            thread: response.thread.into(),
                        });
                    }
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error.to_string());
                        }
                    }
                }
            }

            let reason = first_error
                .map(|error| format!("replacement worker failed to read thread: {error}"))
                .unwrap_or_else(|| "no replacement worker restored the context".to_string());
            self.observability
                .record_account_capacity_event(worker_id, "thread_read_handoff_failure");
            let _ = self
                .events
                .send(GatewayEvent::account_thread_handoff_failed(
                    GatewayAccountThreadHandoffFailed {
                        tenant_id: context.tenant_id.as_str(),
                        project_id: context.project_id.as_deref(),
                        method: "thread/read",
                        thread_id: thread_id.as_str(),
                        exhausted_worker_id: worker_id,
                        exhausted_account_id: self.worker_health.account_id(worker_id),
                        reason: reason.as_str(),
                    },
                ));
            warn!(
                method = "thread/read",
                account_capacity_event = "thread_read_handoff_failure",
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                thread_id = thread_id.as_str(),
                exhausted_worker_id = worker_id,
                exhausted_account_id = self.worker_health.account_id(worker_id),
                reason = reason.as_str(),
                "gateway HTTP failed closed because no replacement account-backed worker restored thread/read"
            );
            return Err(GatewayError::Upstream(reason));
        }

        let response: ThreadReadResponse = worker
            .request_handle()
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
        if request.input.trim().is_empty() {
            return Err(GatewayError::InvalidRequest(
                "turn input must not be empty".to_string(),
            ));
        }
        self.fail_if_active_thread_account_exhausted(&context, "turn/start", &thread_id)?;
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        let response: TurnStartResponse = match worker
            .request_handle()
            .request_typed(turn_start_request(
                self.next_request_id(),
                thread_id,
                request,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if let Some(reason) = account_capacity_error_reason(&error) {
                    self.mark_account_capacity_exhausted(&context, "turn/start", worker_id, reason);
                }
                return Err(GatewayError::from(error));
            }
        };

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
        self.fail_if_active_thread_account_exhausted(&context, "turn/interrupt", &thread_id)?;
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let worker_id = worker.id();
        let _: AppServerTurnInterruptResponse = match worker
            .request_handle()
            .request_typed(turn_interrupt_request(
                self.next_request_id(),
                thread_id,
                turn_id,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if let Some(reason) = account_capacity_error_reason(&error) {
                    self.mark_account_capacity_exhausted(
                        &context,
                        "turn/interrupt",
                        worker_id,
                        reason,
                    );
                }
                return Err(GatewayError::from(error));
            }
        };

        Ok(InterruptTurnResponse { status: "accepted" })
    }

    async fn resolve_server_request(
        &self,
        context: GatewayRequestContext,
        request: ResolveServerRequestRequest,
    ) -> Result<ResolveServerRequestResponse, GatewayError> {
        let Some(pending_request) = self
            .scope_registry
            .pending_server_request(&request.request_id)
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
            let response_kind = request.response.kind().metric_tag();
            self.observability.record_server_request_lifecycle_event(
                "client_server_request_invalid_response",
                response_kind,
            );
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                expected_response_kind = pending_request.kind.metric_tag(),
                response_kind,
                request_id = %request.request_id,
                thread_id = pending_request.thread_id.as_str(),
                "gateway HTTP rejected server request response because its type does not match the pending request"
            );
            return Err(GatewayError::InvalidRequest(
                "server request response type does not match pending request".to_string(),
            ));
        }

        let request_id = request.request_id.clone();
        let response_kind = request.response.kind().metric_tag();
        let Some(worker_id) = pending_request.worker_id else {
            self.record_server_request_delivery_failure(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                "gateway HTTP could not deliver server request response because the pending request has no remote worker route"
            );
            return Err(GatewayError::Upstream(format!(
                "server request {} is missing a remote worker route",
                request.request_id
            )));
        };
        if let Err(error) = self.fail_if_worker_account_exhausted(
            &context,
            "serverRequest/respond",
            &pending_request.thread_id,
            worker_id,
        ) {
            self.record_server_request_delivery_failure(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                worker_id,
                worker_websocket_url = self
                    .workers
                    .get(worker_id)
                    .map(GatewayRemoteWorker::websocket_url)
                    .unwrap_or("unavailable"),
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                error = ?error,
                "gateway HTTP could not deliver server request response because the owning account is exhausted"
            );
            return Err(error);
        }
        let worker = match self.worker(worker_id) {
            Ok(worker) => worker,
            Err(error) => {
                self.record_server_request_delivery_failure(response_kind);
                warn!(
                    tenant_id = context.tenant_id.as_str(),
                    project_id = context.project_id.as_deref(),
                    response_kind,
                    worker_id,
                    worker_websocket_url = self
                        .workers
                        .get(worker_id)
                        .map(GatewayRemoteWorker::websocket_url)
                        .unwrap_or("unavailable"),
                    request_id = %request_id,
                    thread_id = pending_request.thread_id.as_str(),
                    error = ?error,
                    "gateway HTTP could not deliver server request response because the remote worker route is unavailable"
                );
                return Err(error);
            }
        };
        let result = request.response.into_result().map_err(|err| {
            GatewayError::InvalidRequest(format!("invalid server request response payload: {err}"))
        })?;
        self.observability
            .record_server_request_lifecycle_event("client_server_request_answered", response_kind);
        if let Err(err) = worker
            .request_handle()
            .resolve_server_request(request_id.clone(), result)
            .await
        {
            let error = err.to_string();
            self.record_server_request_delivery_failure_after_answer(response_kind);
            warn!(
                tenant_id = context.tenant_id.as_str(),
                project_id = context.project_id.as_deref(),
                response_kind,
                worker_id,
                worker_websocket_url = worker.websocket_url(),
                request_id = %request_id,
                thread_id = pending_request.thread_id.as_str(),
                error = error.as_str(),
                "gateway HTTP failed to deliver server request response to remote worker"
            );
            return Err(GatewayError::Upstream(format!(
                "server request resolve failed: {err}"
            )));
        }
        self.observability.record_server_request_lifecycle_event(
            "client_server_request_delivered",
            response_kind,
        );
        self.scope_registry
            .clear_pending_server_request(&request_id);

        Ok(ResolveServerRequestResponse { status: "accepted" })
    }

    fn health(&self) -> GatewayHealthResponse {
        let remote_workers = self.worker_health.snapshot();
        let remote_unlabeled_account_workers: Vec<GatewayRemoteUnlabeledAccountWorker> =
            if remote_workers.len() > 1 {
                remote_workers
                    .iter()
                    .filter(|worker| {
                        worker
                            .account_id
                            .as_deref()
                            .map(str::trim)
                            .is_none_or(str::is_empty)
                    })
                    .map(|worker| GatewayRemoteUnlabeledAccountWorker {
                        worker_id: worker.worker_id,
                        websocket_url: worker.websocket_url.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let remote_unlabeled_account_worker_ids: Vec<usize> = remote_unlabeled_account_workers
            .iter()
            .map(|worker| worker.worker_id)
            .collect();
        let remote_unlabeled_account_worker_count = remote_unlabeled_account_workers.len();
        let remote_account_labels_complete = remote_unlabeled_account_worker_ids.is_empty();
        let status = if remote_workers.iter().all(|worker| worker.healthy) {
            GatewayHealthStatus::Ok
        } else if remote_workers.iter().any(|worker| worker.healthy) {
            GatewayHealthStatus::Degraded
        } else {
            GatewayHealthStatus::Unavailable
        };

        GatewayHealthResponse {
            status,
            runtime_mode: "remote".to_string(),
            execution_mode: GatewayExecutionMode::WorkerManaged,
            v2_compatibility: if remote_workers.len() == 1 {
                GatewayV2CompatibilityMode::RemoteSingleWorker
            } else {
                GatewayV2CompatibilityMode::RemoteMultiWorker
            },
            v2_transport: self.v2_transport,
            v2_connections: self.v2_connection_health.snapshot(),
            pending_server_request_count: self.scope_registry.pending_server_request_count(),
            pending_server_request_kind_counts: self
                .scope_registry
                .pending_server_request_kind_counts(),
            pending_server_request_route_counts: self
                .scope_registry
                .pending_server_request_route_counts(),
            pending_server_request_oldest_at: self
                .scope_registry
                .pending_server_request_oldest_at(),
            remote_workers: Some(remote_workers),
            remote_account_labels_complete: Some(remote_account_labels_complete),
            remote_unlabeled_account_worker_count: Some(remote_unlabeled_account_worker_count),
            remote_unlabeled_account_worker_ids: Some(remote_unlabeled_account_worker_ids),
            remote_unlabeled_account_workers: Some(remote_unlabeled_account_workers),
            project_worker_routes: Some(self.scope_registry.project_worker_routes(
                |worker_id| self.worker_health.is_healthy(worker_id),
                |worker_id| self.worker_health.account_id(worker_id),
                |worker_id| {
                    self.worker_health
                        .account_capacity(worker_id)
                        .unwrap_or(GatewayAccountCapacityStatus::Exhausted)
                },
            )),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool {
        self.scope_registry.event_visible_to(context, event)
    }
}

fn account_capacity_error_reason(error: &TypedRequestError) -> Option<String> {
    let TypedRequestError::Server { source, .. } = error else {
        return None;
    };

    is_account_capacity_error(source).then(|| source.message.clone())
}

fn is_account_capacity_error(error: &JSONRPCErrorError) -> bool {
    if error.code == 429 {
        return true;
    }

    let message = error.message.to_ascii_lowercase();
    [
        "rate limit",
        "rate_limit",
        "usage limit",
        "usage_limit",
        "credits depleted",
        "billing",
        "quota",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::RemoteWorkerGatewayRuntime;
    use crate::api::GatewayAccountCapacityStatus;
    use crate::api::GatewayExecutionMode;
    use crate::api::GatewayHealthStatus;
    use crate::api::GatewayServerRequestKind;
    use crate::api::GatewayServerRequestResponse;
    use crate::api::GatewaySortDirection;
    use crate::api::GatewayThread;
    use crate::api::GatewayThreadSortKey;
    use crate::api::GatewayThreadStatus;
    use crate::api::GatewayV2CompatibilityMode;
    use crate::api::GatewayV2ConnectionHealth;
    use crate::api::GatewayV2TransportConfig;
    use crate::api::ListThreadsRequest;
    use crate::api::ResolveServerRequestRequest;
    use crate::config::GatewayRemoteSelectionPolicy;
    use crate::error::GatewayError;
    use crate::observability::GatewayObservability;
    use crate::remote_health::RemoteWorkerHealthRegistry;
    use crate::runtime::GatewayRuntime;
    use crate::scope::GatewayRequestContext;
    use crate::scope::GatewayScopeRegistry;
    use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
    use codex_app_server_protocol::CommandExecutionApprovalDecision;
    use codex_app_server_protocol::JSONRPCErrorError;
    use codex_app_server_protocol::RequestId;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::MetricData;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::AtomicI64;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn test_thread(id: &str, created_at: i64, updated_at: i64) -> GatewayThread {
        GatewayThread {
            id: id.to_string(),
            preview: id.to_string(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at,
            updated_at,
            status: GatewayThreadStatus::Idle,
        }
    }

    fn empty_v2_connection_health() -> Arc<GatewayV2ConnectionHealthRegistry> {
        Arc::new(GatewayV2ConnectionHealthRegistry::default())
    }

    fn test_metrics() -> codex_otel::MetricsClient {
        codex_otel::MetricsClient::new(
            codex_otel::MetricsConfig::in_memory(
                "test",
                "codex-gateway",
                env!("CARGO_PKG_VERSION"),
                opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
            )
            .with_runtime_reader(),
        )
        .expect("metrics")
    }

    fn assert_server_request_lifecycle_and_delivery_failure_metrics(
        metrics: &codex_otel::MetricsClient,
        response_kind: &str,
    ) {
        let resource_metrics = metrics.snapshot().expect("snapshot");
        let metrics = resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
        let expected_lifecycle_points = vec![
            (
                BTreeMap::from([
                    (
                        "event".to_string(),
                        "client_server_request_answered".to_string(),
                    ),
                    ("method".to_string(), response_kind.to_string()),
                ]),
                1_u64,
            ),
            (
                BTreeMap::from([
                    (
                        "event".to_string(),
                        "client_server_request_delivery_failed".to_string(),
                    ),
                    ("method".to_string(), response_kind.to_string()),
                ]),
                1_u64,
            ),
        ];
        let mut lifecycle_points = Vec::new();
        let mut saw_delivery_failure_count = false;

        for metric in metrics {
            match metric.name() {
                "gateway_server_request_lifecycle_events" => match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            lifecycle_points.extend(sum.data_points().map(|point| {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                (attributes, point.value())
                            }));
                        }
                        _ => panic!("unexpected server-request lifecycle aggregation"),
                    },
                    _ => panic!("unexpected server-request lifecycle metric type"),
                },
                "gateway_server_request_answer_delivery_failures" => match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            let point = sum.data_points().next().expect("count point");
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            assert_eq!(
                                attributes,
                                BTreeMap::from([(
                                    "response_kind".to_string(),
                                    response_kind.to_string()
                                )])
                            );
                            assert_eq!(point.value(), 1);
                            saw_delivery_failure_count = true;
                        }
                        _ => panic!("unexpected server-request delivery failure aggregation"),
                    },
                    _ => panic!("unexpected server-request delivery failure metric type"),
                },
                _ => {}
            }
        }

        lifecycle_points.sort();
        assert_eq!(lifecycle_points, expected_lifecycle_points);
        assert!(saw_delivery_failure_count);
    }

    fn assert_server_request_lifecycle_metric(
        metrics: &codex_otel::MetricsClient,
        event: &str,
        method: &str,
    ) {
        let resource_metrics = metrics.snapshot().expect("snapshot");
        let metrics = resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
        let expected_attributes = BTreeMap::from([
            ("event".to_string(), event.to_string()),
            ("method".to_string(), method.to_string()),
        ]);
        let mut saw_count = false;

        for metric in metrics {
            if metric.name() == "gateway_server_request_lifecycle_events" {
                match metric.data() {
                    AggregatedMetrics::U64(data) => match data {
                        MetricData::Sum(sum) => {
                            for point in sum.data_points() {
                                let attributes: BTreeMap<String, String> = point
                                    .attributes()
                                    .map(|attribute| {
                                        (
                                            attribute.key.as_str().to_string(),
                                            attribute.value.as_str().to_string(),
                                        )
                                    })
                                    .collect();
                                if attributes == expected_attributes {
                                    assert_eq!(point.value(), 1);
                                    saw_count = true;
                                }
                            }
                        }
                        _ => panic!("unexpected server-request lifecycle aggregation"),
                    },
                    _ => panic!("unexpected server-request lifecycle metric type"),
                }
            }
        }

        assert!(
            saw_count,
            "missing gateway_server_request_lifecycle_events metric point: {expected_attributes:?}"
        );
    }

    #[test]
    fn detects_account_capacity_errors_without_treating_all_server_errors_as_capacity() {
        assert!(super::is_account_capacity_error(&JSONRPCErrorError {
            code: 429,
            data: None,
            message: "too many requests".to_string(),
        }));
        assert!(super::is_account_capacity_error(&JSONRPCErrorError {
            code: -32000,
            data: None,
            message: "workspace_member_usage_limit_reached".to_string(),
        }));
        assert!(!super::is_account_capacity_error(&JSONRPCErrorError {
            code: -32602,
            data: None,
            message: "cwd must be absolute".to_string(),
        }));
    }

    #[test]
    fn sorts_threads_by_created_at_desc_by_default() {
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            worker_health: Arc::new(crate::remote_health::RemoteWorkerHealthRegistry::new(
                Vec::new(),
            )),
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };
        let mut threads = vec![
            test_thread("thread-1", 1, 10),
            test_thread("thread-2", 3, 5),
            test_thread("thread-3", 2, 7),
        ];

        runtime.sort_threads(&mut threads, &ListThreadsRequest::default());

        assert_eq!(
            threads
                .into_iter()
                .map(|thread| thread.id)
                .collect::<Vec<_>>(),
            vec![
                "thread-2".to_string(),
                "thread-3".to_string(),
                "thread-1".to_string()
            ]
        );
    }

    #[test]
    fn sorts_threads_by_updated_at_ascending() {
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            worker_health: Arc::new(crate::remote_health::RemoteWorkerHealthRegistry::new(
                Vec::new(),
            )),
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };
        let mut threads = vec![
            test_thread("thread-1", 1, 10),
            test_thread("thread-2", 3, 5),
            test_thread("thread-3", 2, 7),
        ];

        runtime.sort_threads(
            &mut threads,
            &ListThreadsRequest {
                sort_key: Some(GatewayThreadSortKey::UpdatedAt),
                sort_direction: Some(GatewaySortDirection::Asc),
                ..ListThreadsRequest::default()
            },
        );

        assert_eq!(
            threads
                .into_iter()
                .map(|thread| thread.id)
                .collect::<Vec<_>>(),
            vec![
                "thread-2".to_string(),
                "thread-3".to_string(),
                "thread-1".to_string()
            ]
        );
    }

    #[test]
    fn parses_gateway_thread_cursor_offsets() {
        assert_eq!(
            RemoteWorkerGatewayRuntime::decode_cursor(None).expect("offset"),
            0
        );
        assert_eq!(
            RemoteWorkerGatewayRuntime::decode_cursor(Some("offset:12")).expect("offset"),
            12
        );
    }

    #[test]
    fn rejects_invalid_gateway_thread_cursor_offsets() {
        let error = RemoteWorkerGatewayRuntime::decode_cursor(Some("thread-12"))
            .expect_err("cursor should fail");

        assert!(matches!(
            error,
            GatewayError::InvalidRequest(message) if message == "invalid gateway thread cursor: thread-12"
        ));
    }

    #[test]
    fn preserves_thread_scope_worker_id() {
        let registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        registry.register_thread_with_worker("thread-1".to_string(), context, Some(2));

        assert_eq!(registry.thread_worker_id("thread-1"), Some(2));
    }

    #[test]
    fn active_thread_request_fails_closed_when_account_capacity_is_exhausted() {
        let (events, mut event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread_with_worker(
            "thread-1".to_string(),
            context.clone(),
            Some(1),
        );
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("acct-a".to_string()),
            ),
            (
                "ws://127.0.0.1:8082".to_string(),
                Some("acct-b".to_string()),
            ),
        ]));
        worker_health.mark_account_exhausted_for_worker(1, "quota exceeded".to_string());
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry,
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let error = runtime
            .fail_if_active_thread_account_exhausted(&context, "turn/start", "thread-1")
            .expect_err("active thread request should fail closed");

        assert!(matches!(
            error,
            GatewayError::Upstream(message)
                if message == "thread thread-1 is pinned to worker 1 with exhausted account capacity for turn/start"
        ));
        let event = event_rx.try_recv().expect("handoff event");
        assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
        assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": "turn/start",
                "threadId": "thread-1",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "reason": "thread thread-1 is pinned to worker 1 with exhausted account capacity for turn/start",
            })
        );
    }

    #[test]
    fn downstream_turn_start_capacity_error_marks_account_exhausted() {
        let (events, mut event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        )]));
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        runtime.mark_account_capacity_exhausted(
            &context,
            "turn/start",
            0,
            "rate limit exceeded".to_string(),
        );

        assert_eq!(
            runtime.worker_health.account_capacity(0),
            Some(GatewayAccountCapacityStatus::Exhausted)
        );
        let event = event_rx.try_recv().expect("capacity event");
        assert_eq!(event.method, "gateway/accountCapacityExhausted");
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "workerId": 0,
                "accountId": "acct-a",
                "reason": "rate limit exceeded",
            })
        );
    }

    #[tokio::test]
    async fn interrupt_turn_fails_closed_when_account_capacity_is_exhausted() {
        let (events, mut event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread_with_worker(
            "thread-1".to_string(),
            context.clone(),
            Some(0),
        );
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        )]));
        worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry,
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let error = runtime
            .interrupt_turn(context, "thread-1".to_string(), "turn-1".to_string())
            .await
            .expect_err("interrupt should fail closed before worker lookup");

        assert!(matches!(
            error,
            GatewayError::Upstream(message)
                if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for turn/interrupt"
        ));
        let event = event_rx.try_recv().expect("handoff event");
        assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
        assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": "turn/interrupt",
                "threadId": "thread-1",
                "exhaustedWorkerId": 0,
                "exhaustedAccountId": "acct-a",
                "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for turn/interrupt",
            })
        );
    }

    #[tokio::test]
    async fn server_request_response_fails_closed_when_account_capacity_is_exhausted() {
        let (events, mut event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread_with_worker(
            "thread-1".to_string(),
            context.clone(),
            Some(0),
        );
        scope_registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
            Some(0),
        );
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        )]));
        worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
        let metrics = test_metrics();
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry: scope_registry.clone(),
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::new(Some(metrics.clone()), true),
        };

        let request_id = RequestId::String("req-1".to_string());
        let error = runtime
            .resolve_server_request(
                context,
                ResolveServerRequestRequest {
                    request_id: request_id.clone(),
                    response: GatewayServerRequestResponse::ToolRequestUserInput {
                        answers: HashMap::new(),
                    },
                },
            )
            .await
            .expect_err("server request response should fail closed before worker lookup");

        assert!(matches!(
            error,
            GatewayError::Upstream(message)
                if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        ));
        assert!(scope_registry.pending_server_request(&request_id).is_some());
        let event = event_rx.try_recv().expect("handoff event");
        assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
        assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": "serverRequest/respond",
                "threadId": "thread-1",
                "exhaustedWorkerId": 0,
                "exhaustedAccountId": "acct-a",
                "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
            })
        );
        assert_server_request_lifecycle_and_delivery_failure_metrics(
            &metrics,
            "toolRequestUserInput",
        );
    }

    #[tokio::test]
    async fn server_request_response_fails_closed_using_pending_worker_without_thread_route() {
        let (events, mut event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
            Some(0),
        );
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![(
            "ws://127.0.0.1:8081".to_string(),
            Some("acct-a".to_string()),
        )]));
        worker_health.mark_account_exhausted_for_worker(0, "quota exceeded".to_string());
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry: scope_registry.clone(),
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let request_id = RequestId::String("req-1".to_string());
        let error = runtime
            .resolve_server_request(
                context,
                ResolveServerRequestRequest {
                    request_id: request_id.clone(),
                    response: GatewayServerRequestResponse::ToolRequestUserInput {
                        answers: HashMap::new(),
                    },
                },
            )
            .await
            .expect_err("server request response should fail closed before worker lookup");

        assert!(matches!(
            error,
            GatewayError::Upstream(message)
                if message == "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        ));
        assert!(scope_registry.pending_server_request(&request_id).is_some());
        let event = event_rx.try_recv().expect("handoff event");
        assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
        assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": "serverRequest/respond",
                "threadId": "thread-1",
                "exhaustedWorkerId": 0,
                "exhaustedAccountId": "acct-a",
                "reason": "thread thread-1 is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
            })
        );
    }

    #[tokio::test]
    async fn server_request_response_type_mismatch_records_invalid_response_lifecycle() {
        let (events, _event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
            Some(0),
        );
        let metrics = test_metrics();
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry: scope_registry.clone(),
            worker_health: Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(Vec::new())),
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::new(Some(metrics.clone()), true),
        };

        let request_id = RequestId::String("req-1".to_string());
        let error = runtime
            .resolve_server_request(
                context,
                ResolveServerRequestRequest {
                    request_id: request_id.clone(),
                    response: GatewayServerRequestResponse::CommandExecutionApproval {
                        decision: CommandExecutionApprovalDecision::Decline,
                    },
                },
            )
            .await
            .expect_err("mismatched response type should be rejected");

        assert!(matches!(
            error,
            GatewayError::InvalidRequest(message)
                if message == "server request response type does not match pending request"
        ));
        assert!(scope_registry.pending_server_request(&request_id).is_some());
        assert_server_request_lifecycle_metric(
            &metrics,
            "client_server_request_invalid_response",
            "commandExecutionApproval",
        );
    }

    #[tokio::test]
    async fn server_request_response_keeps_pending_request_when_remote_worker_route_is_missing() {
        let (events, _event_rx) = broadcast::channel(4);
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread_with_worker(
            "thread-1".to_string(),
            context.clone(),
            Some(9),
        );
        scope_registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            context.clone(),
            "thread-1".to_string(),
            Some(9),
        );
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(Vec::new()));
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events,
            scope_registry: scope_registry.clone(),
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let request_id = RequestId::String("req-1".to_string());
        let error = runtime
            .resolve_server_request(
                context,
                ResolveServerRequestRequest {
                    request_id: request_id.clone(),
                    response: GatewayServerRequestResponse::ToolRequestUserInput {
                        answers: HashMap::new(),
                    },
                },
            )
            .await
            .expect_err("server request response should fail before clearing pending request");

        assert!(matches!(
            error,
            GatewayError::Upstream(message)
                if message == "remote worker route 9 is unavailable"
        ));
        let pending_request = scope_registry
            .pending_server_request(&request_id)
            .expect("pending request should still be registered");
        assert_eq!(
            pending_request.kind,
            GatewayServerRequestKind::ToolRequestUserInput
        );
        assert_eq!(
            pending_request.context,
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            }
        );
        assert_eq!(pending_request.thread_id, "thread-1");
        assert_eq!(pending_request.worker_id, Some(9));
        assert!(pending_request.registered_at > 0);
    }

    #[test]
    fn reports_degraded_remote_health_when_some_workers_are_unhealthy() {
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            ("ws://127.0.0.1:8081".to_string(), None),
            (
                "ws://127.0.0.1:8082".to_string(),
                Some("acct-b".to_string()),
            ),
        ]));
        worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            1,
        );
        scope_registry.register_pending_server_request_with_worker(
            RequestId::String("req-1".to_string()),
            GatewayServerRequestKind::ToolRequestUserInput,
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            "thread-1".to_string(),
            Some(1),
        );
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry,
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        assert_eq!(runtime.health().status, GatewayHealthStatus::Degraded);
        let health = runtime.health();
        assert_eq!(health.runtime_mode, "remote".to_string());
        assert_eq!(health.execution_mode, GatewayExecutionMode::WorkerManaged);
        assert_eq!(
            health.v2_compatibility,
            GatewayV2CompatibilityMode::RemoteMultiWorker
        );
        assert_eq!(
            health.v2_transport,
            GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            }
        );
        assert_eq!(health.v2_connections, GatewayV2ConnectionHealth::default());
        assert_eq!(health.pending_server_request_count, 1);
        assert_eq!(
            health.pending_server_request_kind_counts,
            [("toolRequestUserInput".to_string(), 1)].into()
        );
        assert_eq!(
            health.pending_server_request_route_counts,
            vec![crate::api::GatewayPendingServerRequestRouteCounts {
                worker_id: Some(1),
                count: 1,
                kind_counts: [("toolRequestUserInput".to_string(), 1)].into(),
            }]
        );
        assert!(health.pending_server_request_oldest_at.is_some());
        assert_eq!(health.remote_account_labels_complete, Some(false));
        assert_eq!(health.remote_unlabeled_account_worker_count, Some(1));
        assert_eq!(health.remote_unlabeled_account_worker_ids, Some(vec![0]));
        assert_eq!(
            health.remote_unlabeled_account_workers,
            Some(vec![crate::api::GatewayRemoteUnlabeledAccountWorker {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:8081".to_string(),
            }])
        );
        let remote_workers = health.remote_workers.expect("remote workers");
        assert_eq!(remote_workers.len(), 2);
        assert_eq!(
            remote_workers[0],
            crate::api::GatewayRemoteWorkerHealth {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:8081".to_string(),
                account_id: None,
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                account_capacity_last_changed_at: None,
                healthy: true,
                reconnecting: false,
                reconnect_attempt_count: 0,
                last_error: None,
                last_state_change_at: None,
                last_error_at: None,
                next_reconnect_at: None,
                reconnect_backoff_remaining_seconds: None,
            }
        );
        assert_eq!(remote_workers[1].worker_id, 1);
        assert_eq!(remote_workers[1].websocket_url, "ws://127.0.0.1:8082");
        assert_eq!(remote_workers[1].account_id, Some("acct-b".to_string()));
        assert_eq!(remote_workers[1].healthy, false);
        assert_eq!(remote_workers[1].reconnecting, false);
        assert_eq!(remote_workers[1].reconnect_attempt_count, 0);
        assert_eq!(
            remote_workers[1].last_error.as_deref(),
            Some("socket closed")
        );
        assert_eq!(remote_workers[1].last_state_change_at.is_some(), true);
        assert_eq!(remote_workers[1].last_error_at.is_some(), true);
        assert_eq!(remote_workers[1].next_reconnect_at, None);
        assert_eq!(
            health.project_worker_routes,
            Some(vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: false,
            }])
        );
    }

    #[test]
    fn reports_complete_remote_account_labels_for_labeled_multi_worker_health() {
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("acct-a".to_string()),
            ),
            (
                "ws://127.0.0.1:8082".to_string(),
                Some("acct-b".to_string()),
            ),
        ]));
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            1,
        );
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry,
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let health = runtime.health();

        assert_eq!(
            health.v2_compatibility,
            GatewayV2CompatibilityMode::RemoteMultiWorker
        );
        assert_eq!(health.remote_account_labels_complete, Some(true));
        assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
        assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
        assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
        let remote_workers = health.remote_workers.expect("remote workers");
        assert_eq!(
            remote_workers
                .iter()
                .map(|worker| worker.account_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("acct-a"), Some("acct-b")]
        );
        assert_eq!(
            health.project_worker_routes,
            Some(vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
            }])
        );
    }

    #[test]
    fn project_worker_routes_track_worker_health_and_capacity_changes() {
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("acct-a".to_string()),
            ),
            (
                "ws://127.0.0.1:8082".to_string(),
                Some("acct-b".to_string()),
            ),
        ]));
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_project_worker(
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
            1,
        );
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry,
            worker_health: worker_health.clone(),
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        runtime
            .v2_connection_health
            .record_project_worker_route_selected(1, "tenant-a", "project-a", "thread-a");

        let healthy_snapshot = runtime.health();
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .project_worker_route_selection_count,
            1
        );
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .project_worker_route_selection_worker_counts,
            vec![
                crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                    worker_id: 1,
                    project_worker_route_selection_count: 1,
                }
            ]
        );
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .last_project_worker_route_selected_worker_id,
            Some(1)
        );
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .last_project_worker_route_selected_tenant_id
                .as_deref(),
            Some("tenant-a")
        );
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .last_project_worker_route_selected_project_id
                .as_deref(),
            Some("project-a")
        );
        assert_eq!(
            healthy_snapshot
                .v2_connections
                .last_project_worker_route_selected_thread_id
                .as_deref(),
            Some("thread-a")
        );
        assert!(
            healthy_snapshot
                .v2_connections
                .last_project_worker_route_selected_at
                .is_some()
        );
        assert_eq!(
            healthy_snapshot.project_worker_routes,
            Some(vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
            }])
        );

        worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
        worker_health.mark_account_exhausted_for_worker(1, "quota exceeded".to_string());

        let degraded_snapshot = runtime.health();
        assert_eq!(
            degraded_snapshot
                .v2_connections
                .project_worker_route_selection_count,
            1
        );
        assert_eq!(
            degraded_snapshot.project_worker_routes,
            Some(vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
                worker_healthy: false,
            }])
        );

        worker_health.mark_healthy(1);
        worker_health.mark_account_available_for_worker(1);

        let recovered_snapshot = runtime.health();
        assert_eq!(
            recovered_snapshot
                .v2_connections
                .project_worker_route_selection_count,
            1
        );
        assert_eq!(
            recovered_snapshot.project_worker_routes,
            Some(vec![crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
            }])
        );
    }

    #[test]
    fn reports_reconnecting_remote_health_with_retry_metadata() {
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new(vec![
            "ws://127.0.0.1:8081".to_string(),
        ]));
        worker_health.mark_reconnecting(
            0,
            Some("remote app server event stream ended".to_string()),
            Duration::from_millis(250),
        );
        let runtime = RemoteWorkerGatewayRuntime {
            workers: Vec::new(),
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            next_worker: AtomicUsize::new(0),
            next_request_id: Arc::new(AtomicI64::new(1)),
            events: broadcast::channel(4).0,
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            worker_health,
            v2_transport: GatewayV2TransportConfig {
                initialize_timeout_seconds: 30,
                client_send_timeout_seconds: 10,
                reconnect_retry_backoff_seconds: 1,
                max_pending_server_requests: 64,
                max_pending_client_requests: 64,
            },
            v2_connection_health: empty_v2_connection_health(),
            observability: GatewayObservability::default(),
        };

        let health = runtime.health();

        assert_eq!(health.status, GatewayHealthStatus::Unavailable);
        assert_eq!(
            health.v2_compatibility,
            GatewayV2CompatibilityMode::RemoteSingleWorker
        );
        assert_eq!(health.remote_account_labels_complete, Some(true));
        assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
        assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
        assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
        let remote_workers = health.remote_workers.expect("remote workers");
        assert_eq!(remote_workers.len(), 1);
        assert_eq!(remote_workers[0].worker_id, 0);
        assert_eq!(remote_workers[0].websocket_url, "ws://127.0.0.1:8081");
        assert_eq!(remote_workers[0].healthy, false);
        assert_eq!(remote_workers[0].reconnecting, true);
        assert_eq!(remote_workers[0].reconnect_attempt_count, 0);
        assert_eq!(
            remote_workers[0].last_error.as_deref(),
            Some("remote app server event stream ended")
        );
        assert_eq!(remote_workers[0].last_state_change_at.is_some(), true);
        assert_eq!(remote_workers[0].last_error_at.is_some(), true);
        assert_eq!(remote_workers[0].next_reconnect_at.is_some(), true);
        assert_eq!(
            remote_workers[0]
                .reconnect_backoff_remaining_seconds
                .is_some_and(|remaining_seconds| remaining_seconds >= 0),
            true
        );
    }
}
