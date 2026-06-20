use crate::northbound::v2::GatewayV2Timeouts;
use crate::northbound::v2_connection::DownstreamWorkerEvent;
use crate::northbound::v2_connection::DownstreamWorkerHandle;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::GatewayV2ReconnectState;
use crate::northbound::v2_connection_runtime::spawn_downstream_worker_session;
use crate::northbound::v2_wire::downstream_protocol_violation_reason;
use crate::northbound::v2_wire::log_downstream_reconnect_protocol_violation;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::v2::GatewayV2SessionFactory;
use codex_app_server_protocol::InitializeParams;
use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::info;
use tracing::warn;

impl GatewayV2DownstreamRouter {
    #[cfg(test)]
    pub(crate) async fn connect(
        session_factory: &GatewayV2SessionFactory,
        initialize_params: &InitializeParams,
        request_context: &GatewayRequestContext,
    ) -> io::Result<Self> {
        Self::connect_with_timeouts(
            session_factory,
            initialize_params,
            request_context,
            &GatewayV2Timeouts::default(),
        )
        .await
    }

    pub(crate) async fn connect_with_timeouts(
        session_factory: &GatewayV2SessionFactory,
        initialize_params: &InitializeParams,
        request_context: &GatewayRequestContext,
        timeouts: &GatewayV2Timeouts,
    ) -> io::Result<Self> {
        info!("gateway downstream router connect starting");
        let sessions = session_factory
            .connect(initialize_params, request_context)
            .await?;
        info!(
            session_count = sessions.len(),
            "gateway downstream router connect finished"
        );
        let (event_tx, event_rx) = mpsc::channel(sessions.len().max(1) * 4);
        let reconnect_state = match session_factory {
            GatewayV2SessionFactory::RemoteSingle { connect_args, .. } => {
                Some(GatewayV2ReconnectState {
                    configured_worker_ids: vec![0],
                    worker_websocket_urls: vec![connect_args.websocket_url().to_string()],
                    session_factory: session_factory.clone(),
                    initialize_params: initialize_params.clone(),
                    request_context: request_context.clone(),
                    retry_backoff: timeouts.reconnect_retry_backoff,
                })
            }
            GatewayV2SessionFactory::RemoteMulti { connect_args, .. } => {
                Some(GatewayV2ReconnectState {
                    configured_worker_ids: (0..connect_args.len()).collect(),
                    worker_websocket_urls: connect_args
                        .iter()
                        .map(|args| args.websocket_url().to_string())
                        .collect(),
                    session_factory: session_factory.clone(),
                    initialize_params: initialize_params.clone(),
                    request_context: request_context.clone(),
                    retry_backoff: timeouts.reconnect_retry_backoff,
                })
            }
            _ => None,
        };
        let mut router = Self {
            workers: Vec::with_capacity(sessions.len()),
            event_tx,
            event_rx,
            shutdown_txs: Vec::with_capacity(sessions.len()),
            event_tasks: Vec::with_capacity(sessions.len()),
            next_worker: 0,
            initialized_notification_sent: false,
            active_fs_watches: HashMap::new(),
            reconnect_retry_after: HashMap::new(),
            reconnect_state,
        };

        for session in sessions {
            router.add_session(session);
        }

        Ok(router)
    }

    pub(crate) fn add_session(&mut self, session: crate::v2::GatewayV2ConnectedSession) {
        let (worker, shutdown_tx, event_task) =
            spawn_downstream_worker_session(self.event_tx.clone(), session);
        if self.workers.len() != self.shutdown_txs.len()
            || self.workers.len() != self.event_tasks.len()
        {
            self.workers.push(worker);
            self.shutdown_txs.push(shutdown_tx);
            self.event_tasks.push(event_task);
            return;
        }
        let mut sessions = self
            .workers
            .drain(..)
            .zip(self.shutdown_txs.drain(..))
            .zip(self.event_tasks.drain(..))
            .map(|((worker, shutdown_tx), event_task)| (worker, shutdown_tx, event_task))
            .collect::<Vec<_>>();
        sessions.push((worker, shutdown_tx, event_task));
        sessions.sort_by_key(|(worker, _, _)| worker.worker_id.unwrap_or(usize::MAX));
        for (worker, shutdown_tx, event_task) in sessions {
            self.workers.push(worker);
            self.shutdown_txs.push(shutdown_tx);
            self.event_tasks.push(event_task);
        }
    }

    pub(crate) fn single_worker(&self) -> bool {
        self.workers.len() == 1
    }

    pub(crate) fn latest_worker_with_id(
        &self,
        worker_id: Option<usize>,
    ) -> Option<&DownstreamWorkerHandle> {
        self.workers
            .iter()
            .rev()
            .find(|worker| worker.worker_id == worker_id)
    }

    pub(crate) fn multi_worker_topology(&self) -> bool {
        self.reconnect_state
            .as_ref()
            .is_some_and(|state| state.configured_worker_ids.len() > 1)
            || self.workers.len() > 1
    }

    pub(crate) fn primary_worker(&self) -> io::Result<&DownstreamWorkerHandle> {
        if self.multi_worker_topology() {
            return self.latest_worker_with_id(Some(0)).ok_or_else(|| {
                io::Error::other("gateway v2 connection has no downstream app-server sessions")
            });
        }

        self.workers.first().ok_or_else(|| {
            io::Error::other("gateway v2 connection has no downstream app-server sessions")
        })
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub(crate) fn has_worker(&self, worker_id: Option<usize>) -> bool {
        self.workers
            .iter()
            .any(|worker| worker.worker_id == worker_id)
    }

    fn next_thread_start_worker(&mut self) -> io::Result<&DownstreamWorkerHandle> {
        let Some(index) = self.next_worker_index_with_health_and_account_capacity() else {
            return Err(io::Error::other(
                "gateway v2 connection has no healthy downstream app-server sessions with available account capacity",
            ));
        };
        Ok(&self.workers[index])
    }

    fn next_worker_index_with_health_and_account_capacity(&mut self) -> Option<usize> {
        if self.workers.is_empty() {
            return None;
        }

        let start = self.next_worker;
        self.next_worker = self.next_worker.wrapping_add(1);
        (0..self.workers.len())
            .map(|offset| (start + offset) % self.workers.len())
            .find(|index| {
                self.workers[*index].worker_id.is_none_or(|worker_id| {
                    self.worker_is_healthy(worker_id) && self.worker_account_has_capacity(worker_id)
                })
            })
    }

    pub(crate) fn next_thread_start_worker_for_project(
        &mut self,
        scope_registry: &crate::scope::GatewayScopeRegistry,
        context: &GatewayRequestContext,
    ) -> io::Result<&DownstreamWorkerHandle> {
        if let Some(worker_id) = scope_registry.select_project_worker_id_with_accounts(
            context,
            self.workers
                .iter()
                .filter_map(|worker| worker.worker_id)
                .filter(|worker_id| {
                    self.worker_is_healthy(*worker_id)
                        && self.worker_account_has_capacity(*worker_id)
                }),
            |worker_id| self.worker_account_id(worker_id),
        ) && let Some(index) = self
            .workers
            .iter()
            .rposition(|worker| worker.worker_id == Some(worker_id))
        {
            return Ok(&self.workers[index]);
        }
        self.next_thread_start_worker()
    }

    pub(crate) fn worker_for_thread(
        &self,
        scope_registry: &crate::scope::GatewayScopeRegistry,
        thread_id: &str,
    ) -> io::Result<&DownstreamWorkerHandle> {
        let worker_id = scope_registry.thread_worker_id(thread_id);
        self.latest_worker_with_id(worker_id)
            .or_else(|| self.latest_worker_with_id(None))
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    format!("gateway has no downstream worker route for thread {thread_id}"),
                )
            })
    }

    pub(crate) async fn next_event(&mut self) -> Option<DownstreamWorkerEvent> {
        self.event_rx.recv().await
    }

    pub(crate) async fn shutdown(self) -> io::Result<()> {
        for shutdown_tx in self.shutdown_txs {
            let _ = shutdown_tx.send(());
        }
        for event_task in self.event_tasks {
            event_task.await.map_err(|err| {
                io::Error::other(format!("gateway v2 event task failed: {err}"))
            })??;
        }
        Ok(())
    }

    pub(crate) fn remove_worker(&mut self, worker_id: Option<usize>) -> bool {
        let Some(index) = self
            .workers
            .iter()
            .position(|worker| worker.worker_id == worker_id)
        else {
            return false;
        };
        self.workers.remove(index);
        if index < self.shutdown_txs.len() {
            self.shutdown_txs.remove(index);
        }
        if index < self.event_tasks.len() {
            self.event_tasks.remove(index);
        }
        if self.next_worker >= self.workers.len() && !self.workers.is_empty() {
            self.next_worker %= self.workers.len();
        }
        true
    }

    pub(crate) async fn reconnect_missing_workers(&mut self, observability: &GatewayObservability) {
        self.reconnect_missing_workers_at(Instant::now(), observability, true)
            .await;
    }

    pub(crate) async fn reconnect_missing_workers_for_request(
        &mut self,
        observability: &GatewayObservability,
    ) {
        if let Some(delay) = self.shortest_request_reconnect_backoff(Instant::now()) {
            tokio::time::sleep(delay).await;
        }
        self.reconnect_missing_workers_at(Instant::now(), observability, true)
            .await;
    }

    pub(crate) async fn reconnect_missing_workers_after_disconnect(
        &mut self,
        observability: &GatewayObservability,
    ) {
        self.reconnect_missing_workers_at(Instant::now(), observability, false)
            .await;
    }

    pub(crate) async fn reconnect_missing_workers_at(
        &mut self,
        now: Instant,
        observability: &GatewayObservability,
        respect_backoff: bool,
    ) {
        let Some(reconnect_state) = self.reconnect_state.clone() else {
            return;
        };
        if self.workers.len() >= reconnect_state.configured_worker_ids.len() {
            return;
        }

        for worker_id in reconnect_state.configured_worker_ids {
            if self.has_worker(Some(worker_id)) {
                continue;
            }
            let websocket_url = reconnect_state
                .worker_websocket_urls
                .get(worker_id)
                .map_or("<unknown>", String::as_str);
            if respect_backoff && !self.should_attempt_worker_reconnect(worker_id, now) {
                observability.record_v2_worker_reconnect(worker_id, "backoff_suppressed");
                let reconnect_backoff_remaining_seconds = self
                    .reconnect_retry_after
                    .get(&worker_id)
                    .filter(|retry_after| **retry_after > now)
                    .map(|retry_after| retry_after.duration_since(now).as_secs());
                warn!(
                    worker_id,
                    websocket_url,
                    initialized_notification_sent = self.initialized_notification_sent,
                    active_fs_watch_count = self.active_fs_watches.len(),
                    retry_backoff_seconds = reconnect_state.retry_backoff.as_secs(),
                    reconnect_backoff_remaining_seconds,
                    "suppressing missing downstream worker reconnect while retry backoff is active"
                );
                continue;
            }
            let mut last_error = None;
            for attempt_index in 0..3 {
                observability.record_v2_worker_reconnect(worker_id, "attempt");
                info!(
                    worker_id,
                    websocket_url,
                    attempt_index,
                    initialized_notification_sent = self.initialized_notification_sent,
                    active_fs_watch_count = self.active_fs_watches.len(),
                    retry_backoff_seconds = reconnect_state.retry_backoff.as_secs(),
                    "attempting to reconnect missing downstream worker session"
                );
                match reconnect_state
                    .session_factory
                    .connect_worker_once(
                        worker_id,
                        &reconnect_state.initialize_params,
                        &reconnect_state.request_context,
                    )
                    .await
                {
                    Ok(session) => {
                        if let Err(err) = self.replay_connection_state(&session).await {
                            let err_message = err.to_string();
                            last_error = Some(err);
                            observability.record_v2_worker_reconnect(worker_id, "replay_failure");
                            warn!(
                                worker_id,
                                websocket_url,
                                attempt_index,
                                initialized_notification_sent = self.initialized_notification_sent,
                                active_fs_watch_count = self.active_fs_watches.len(),
                                retry_backoff_seconds = reconnect_state.retry_backoff.as_secs(),
                                err = %err_message,
                                "failed to replay connection state to reconnected downstream worker session"
                            );
                            continue;
                        }
                        self.clear_worker_reconnect_failure(worker_id);
                        reconnect_state
                            .session_factory
                            .mark_worker_healthy(worker_id);
                        observability.record_v2_worker_reconnect(worker_id, "success");
                        info!(
                            worker_id,
                            websocket_url,
                            attempt_index,
                            initialized_notification_sent = self.initialized_notification_sent,
                            active_fs_watch_count = self.active_fs_watches.len(),
                            "reconnected missing downstream worker session"
                        );
                        self.add_session(session);
                        last_error = None;
                        break;
                    }
                    Err(err) => {
                        let detail = err.to_string();
                        if let Some(reason) = downstream_protocol_violation_reason(&detail) {
                            log_downstream_reconnect_protocol_violation(
                                &reconnect_state.request_context,
                                worker_id,
                                websocket_url,
                                reason,
                                &detail,
                            );
                            observability.record_v2_worker_protocol_violation(
                                Some(worker_id),
                                "downstream",
                                reason,
                            );
                        }
                        observability.record_v2_worker_reconnect(worker_id, "connect_failure");
                        warn!(
                            worker_id,
                            websocket_url,
                            attempt_index,
                            initialized_notification_sent = self.initialized_notification_sent,
                            active_fs_watch_count = self.active_fs_watches.len(),
                            retry_backoff_seconds = reconnect_state.retry_backoff.as_secs(),
                            err = %detail,
                            "failed to reconnect missing downstream worker session"
                        );
                        last_error = Some(err);
                    }
                }
            }
            if last_error.is_some() && !self.has_worker(Some(worker_id)) {
                self.record_worker_reconnect_failure(worker_id, now, reconnect_state.retry_backoff);
            }
        }
    }
}
