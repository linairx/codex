use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection::UnavailableWorkerRouteDiagnostics;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::FsWatchParams;
use codex_app_server_protocol::FsWatchResponse;
use codex_app_server_protocol::RequestId;
use std::io;
use std::time::Duration;
use std::time::Instant;

impl GatewayV2DownstreamRouter {
    pub(crate) fn mark_initialized(&mut self) {
        self.initialized_notification_sent = true;
    }

    pub(crate) fn record_fs_watch(&mut self, params: FsWatchParams) {
        self.active_fs_watches
            .insert(params.watch_id.clone(), params);
    }

    pub(crate) fn clear_fs_watch(&mut self, watch_id: &str) {
        self.active_fs_watches.remove(watch_id);
    }

    pub(crate) fn should_attempt_worker_reconnect(&self, worker_id: usize, now: Instant) -> bool {
        self.reconnect_retry_after
            .get(&worker_id)
            .is_none_or(|retry_after| *retry_after <= now)
    }

    pub(crate) fn shortest_request_reconnect_backoff(&self, now: Instant) -> Option<Duration> {
        let reconnect_state = self.reconnect_state.as_ref()?;
        self.reconnect_retry_after
            .iter()
            .filter(|(worker_id, retry_after)| {
                reconnect_state.configured_worker_ids.contains(worker_id) && **retry_after > now
            })
            .filter_map(|(_, retry_after)| {
                let delay = retry_after.duration_since(now);
                (delay <= reconnect_state.retry_backoff).then_some(delay)
            })
            .min()
    }

    pub(crate) fn record_worker_reconnect_failure(
        &mut self,
        worker_id: usize,
        now: Instant,
        retry_backoff: Duration,
    ) {
        self.reconnect_retry_after
            .insert(worker_id, now + retry_backoff);
    }

    pub(crate) fn clear_worker_reconnect_failure(&mut self, worker_id: usize) {
        self.reconnect_retry_after.remove(&worker_id);
    }

    pub(crate) fn ensure_all_configured_workers_present_for(&self, label: &str) -> io::Result<()> {
        let Some(reconnect_state) = &self.reconnect_state else {
            return Ok(());
        };

        let missing_worker_ids = reconnect_state
            .configured_worker_ids
            .iter()
            .copied()
            .filter(|worker_id| !self.has_worker(Some(*worker_id)))
            .collect::<Vec<_>>();
        if missing_worker_ids.is_empty() {
            return Ok(());
        }

        Err(io::Error::other(FailClosedMultiWorkerRouteError {
            message: format!(
                "required worker routes are unavailable for {label}: {missing_worker_ids:?}"
            ),
        }))
    }

    pub(crate) fn ensure_primary_worker_present_for(&self, label: &str) -> io::Result<()> {
        if self.has_worker(Some(0)) {
            return Ok(());
        }

        Err(io::Error::other(FailClosedMultiWorkerRouteError {
            message: format!("primary worker route is unavailable for {label}"),
        }))
    }

    pub(crate) fn unavailable_worker_route_diagnostics(
        &self,
        now: Instant,
    ) -> Vec<UnavailableWorkerRouteDiagnostics> {
        let Some(reconnect_state) = &self.reconnect_state else {
            return Vec::new();
        };

        reconnect_state
            .configured_worker_ids
            .iter()
            .copied()
            .filter(|worker_id| !self.has_worker(Some(*worker_id)))
            .map(|worker_id| {
                let reconnect_backoff_remaining_seconds = self
                    .reconnect_retry_after
                    .get(&worker_id)
                    .filter(|retry_after| **retry_after > now)
                    .map(|retry_after| retry_after.duration_since(now).as_secs());
                UnavailableWorkerRouteDiagnostics {
                    worker_id,
                    websocket_url: reconnect_state
                        .worker_websocket_urls
                        .get(worker_id)
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    reconnect_backoff_active: reconnect_backoff_remaining_seconds.is_some(),
                    reconnect_backoff_remaining_seconds,
                }
            })
            .collect()
    }

    pub(crate) fn websocket_url_for_worker_id(&self, worker_id: Option<usize>) -> &str {
        if let Some(worker_websocket_url) = self
            .workers
            .iter()
            .rev()
            .find(|worker| worker.worker_id == worker_id)
            .and_then(|worker| worker.worker_websocket_url.as_deref())
        {
            return worker_websocket_url;
        }

        worker_id
            .and_then(|worker_id| {
                self.reconnect_state
                    .as_ref()
                    .and_then(|state| state.worker_websocket_urls.get(worker_id))
            })
            .map_or("<unknown>", String::as_str)
    }

    pub(crate) fn worker_account_id(&self, worker_id: usize) -> Option<String> {
        self.reconnect_state
            .as_ref()
            .and_then(|state| state.session_factory.worker_account_id(worker_id))
    }

    pub(crate) fn worker_account_has_capacity(&self, worker_id: usize) -> bool {
        self.reconnect_state
            .as_ref()
            .is_none_or(|state| state.session_factory.worker_account_has_capacity(worker_id))
    }

    pub(crate) fn worker_is_healthy(&self, worker_id: usize) -> bool {
        self.reconnect_state
            .as_ref()
            .is_none_or(|state| state.session_factory.worker_is_healthy(worker_id))
    }

    pub(crate) fn mark_worker_account_exhausted(&self, worker_id: usize, reason: String) -> bool {
        if let Some(state) = &self.reconnect_state {
            return state
                .session_factory
                .mark_worker_account_exhausted(worker_id, reason);
        }
        false
    }

    pub(crate) fn mark_worker_account_available(&self, worker_id: usize) -> bool {
        if let Some(state) = &self.reconnect_state {
            return state
                .session_factory
                .mark_worker_account_available(worker_id);
        }
        false
    }

    pub(crate) async fn replay_connection_state(
        &self,
        session: &crate::v2::GatewayV2ConnectedSession,
    ) -> io::Result<()> {
        let request_handle = session.app_server.request_handle();
        if self.initialized_notification_sent {
            request_handle
                .notify(ClientNotification::Initialized)
                .await?;
        }
        for params in self.active_fs_watches.values() {
            request_handle
                .request_typed::<FsWatchResponse>(ClientRequest::FsWatch {
                    request_id: RequestId::String(format!(
                        "gateway-replay-fs-watch:{watch_id}",
                        watch_id = params.watch_id
                    )),
                    params: params.clone(),
                })
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
}
