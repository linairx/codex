use crate::adapter::thread_list_request;
use crate::adapter::thread_read_request;
use crate::adapter::thread_start_request;
use crate::adapter::turn_interrupt_request;
use crate::adapter::turn_start_request;
use crate::api::CreateThreadRequest;
use crate::api::GatewayExecutionMode;
use crate::api::GatewayHealthResponse;
use crate::api::GatewayHealthStatus;
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
use crate::event::GatewayEvent;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_worker::GatewayRemoteWorker;
use crate::runtime::GatewayRuntime;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use async_trait::async_trait;
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
}

impl RemoteWorkerGatewayRuntime {
    pub(crate) fn new(
        workers: Vec<GatewayRemoteWorker>,
        selection_policy: GatewayRemoteSelectionPolicy,
        events: broadcast::Sender<GatewayEvent>,
        scope_registry: Arc<GatewayScopeRegistry>,
        worker_health: Arc<RemoteWorkerHealthRegistry>,
        v2_transport: GatewayV2TransportConfig,
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
                    if self.worker_health.is_healthy(index) {
                        return Ok(&self.workers[index]);
                    }
                }

                Err(GatewayError::Upstream(
                    "no healthy remote workers are available".to_string(),
                ))
            }
        }
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
        for _ in 0..self.workers.len() {
            let worker = self.pick_worker()?;
            match worker
                .request_handle()
                .request_typed::<codex_app_server_protocol::ThreadStartResponse>(
                    thread_start_request(self.next_request_id(), request.clone()),
                )
                .await
            {
                Ok(response) => {
                    self.scope_registry.register_thread_with_worker(
                        response.thread.id.clone(),
                        context.clone(),
                        Some(worker.id()),
                    );

                    return Ok(ThreadResponse {
                        thread: response.thread.into(),
                    });
                }
                Err(error) => {
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
            "no healthy remote workers are available".to_string(),
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
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let response: TurnStartResponse = worker
            .request_handle()
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
        let worker = self.worker_for_thread(&context, &thread_id)?;
        let _: AppServerTurnInterruptResponse = worker
            .request_handle()
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

        let Some(worker_id) = pending_request.worker_id else {
            return Err(GatewayError::Upstream(format!(
                "server request {} is missing a remote worker route",
                request.request_id
            )));
        };
        let worker = self.worker(worker_id)?;
        let result = request.response.into_result().map_err(|err| {
            GatewayError::InvalidRequest(format!("invalid server request response payload: {err}"))
        })?;
        worker
            .request_handle()
            .resolve_server_request(request.request_id, result)
            .await
            .map_err(|err| {
                GatewayError::Upstream(format!("server request resolve failed: {err}"))
            })?;

        Ok(ResolveServerRequestResponse { status: "accepted" })
    }

    fn health(&self) -> GatewayHealthResponse {
        let remote_workers = self.worker_health.snapshot();
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
            remote_workers: Some(remote_workers),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.events.subscribe()
    }

    fn event_visible_to(&self, context: &GatewayRequestContext, event: &GatewayEvent) -> bool {
        self.scope_registry.event_visible_to(context, event)
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteWorkerGatewayRuntime;
    use crate::api::GatewayExecutionMode;
    use crate::api::GatewayHealthResponse;
    use crate::api::GatewayHealthStatus;
    use crate::api::GatewaySortDirection;
    use crate::api::GatewayThread;
    use crate::api::GatewayThreadSortKey;
    use crate::api::GatewayThreadStatus;
    use crate::api::GatewayV2CompatibilityMode;
    use crate::api::GatewayV2TransportConfig;
    use crate::api::ListThreadsRequest;
    use crate::config::GatewayRemoteSelectionPolicy;
    use crate::error::GatewayError;
    use crate::runtime::GatewayRuntime;
    use crate::scope::GatewayRequestContext;
    use crate::scope::GatewayScopeRegistry;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicI64;
    use std::sync::atomic::AtomicUsize;
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
                max_pending_server_requests: 64,
            },
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
                max_pending_server_requests: 64,
            },
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
    fn reports_degraded_remote_health_when_some_workers_are_unhealthy() {
        let worker_health = Arc::new(crate::remote_health::RemoteWorkerHealthRegistry::new(vec![
            "ws://127.0.0.1:8081".to_string(),
            "ws://127.0.0.1:8082".to_string(),
        ]));
        worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
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
                max_pending_server_requests: 64,
            },
        };

        assert_eq!(
            runtime.health(),
            GatewayHealthResponse {
                status: GatewayHealthStatus::Degraded,
                runtime_mode: "remote".to_string(),
                execution_mode: GatewayExecutionMode::WorkerManaged,
                v2_compatibility: GatewayV2CompatibilityMode::RemoteMultiWorker,
                v2_transport: GatewayV2TransportConfig {
                    initialize_timeout_seconds: 30,
                    client_send_timeout_seconds: 10,
                    max_pending_server_requests: 64,
                },
                remote_workers: Some(vec![
                    crate::api::GatewayRemoteWorkerHealth {
                        worker_id: 0,
                        websocket_url: "ws://127.0.0.1:8081".to_string(),
                        healthy: true,
                        last_error: None,
                    },
                    crate::api::GatewayRemoteWorkerHealth {
                        worker_id: 1,
                        websocket_url: "ws://127.0.0.1:8082".to_string(),
                        healthy: false,
                        last_error: Some("socket closed".to_string()),
                    },
                ]),
            }
        );
    }
}
