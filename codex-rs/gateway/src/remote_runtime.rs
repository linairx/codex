use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayV2TransportConfig;
use crate::config::GatewayRemoteSelectionPolicy;
use crate::error::GatewayError;
use crate::event::GatewayAccountActiveThreadHandoffFailed;
use crate::event::GatewayEvent;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::remote_worker::GatewayRemoteWorker;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2_connection_health::GatewayV2ConnectionHealthRegistry;
use codex_app_server_protocol::RequestId;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;
use tracing::warn;

#[cfg(test)]
use codex_app_server_protocol::JSONRPCErrorError;

#[path = "remote_runtime_requests.rs"]
mod remote_runtime_requests;
#[path = "remote_runtime_server_request.rs"]
mod remote_runtime_server_request;
#[path = "remote_runtime_views.rs"]
mod remote_runtime_views;

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
}

#[cfg(test)]
fn is_account_capacity_error(error: &JSONRPCErrorError) -> bool {
    remote_runtime_views::is_account_capacity_error(error)
}

#[cfg(test)]
#[path = "remote_runtime_tests.rs"]
mod tests;
