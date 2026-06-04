use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayRemoteWorkerHealth;
use crate::config::normalize_remote_account_id;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteWorkerHealthState {
    websocket_url: String,
    account_id: Option<String>,
    account_capacity: GatewayAccountCapacityStatus,
    account_capacity_reason: Option<String>,
    account_capacity_last_changed_at: Option<i64>,
    healthy: bool,
    reconnecting: bool,
    reconnect_attempt_count: u32,
    last_error: Option<String>,
    last_state_change_at: Option<i64>,
    last_error_at: Option<i64>,
    next_reconnect_at: Option<i64>,
}

pub struct RemoteWorkerHealthRegistry {
    workers: RwLock<Vec<RemoteWorkerHealthState>>,
}

impl RemoteWorkerHealthRegistry {
    pub fn new(websocket_urls: Vec<String>) -> Self {
        Self::new_with_accounts(
            websocket_urls
                .into_iter()
                .map(|websocket_url| (websocket_url, None))
                .collect(),
        )
    }

    pub fn new_with_accounts(workers: Vec<(String, Option<String>)>) -> Self {
        Self {
            workers: RwLock::new(
                workers
                    .into_iter()
                    .map(|(websocket_url, account_id)| RemoteWorkerHealthState {
                        websocket_url,
                        account_id: normalize_remote_account_id(account_id),
                        account_capacity: GatewayAccountCapacityStatus::Available,
                        account_capacity_reason: None,
                        account_capacity_last_changed_at: None,
                        healthy: true,
                        reconnecting: false,
                        reconnect_attempt_count: 0,
                        last_error: None,
                        last_state_change_at: None,
                        last_error_at: None,
                        next_reconnect_at: None,
                    })
                    .collect(),
            ),
        }
    }

    pub fn is_healthy(&self, worker_id: usize) -> bool {
        read_guard(&self.workers)
            .get(worker_id)
            .is_some_and(|worker| worker.healthy)
    }

    pub fn account_id(&self, worker_id: usize) -> Option<String> {
        read_guard(&self.workers)
            .get(worker_id)
            .and_then(|worker| worker.account_id.clone())
    }

    pub fn account_capacity(&self, worker_id: usize) -> Option<GatewayAccountCapacityStatus> {
        read_guard(&self.workers)
            .get(worker_id)
            .map(|worker| worker.account_capacity)
    }

    pub fn account_has_capacity(&self, worker_id: usize) -> bool {
        self.account_capacity(worker_id) == Some(GatewayAccountCapacityStatus::Available)
    }

    pub fn mark_account_exhausted_for_worker(&self, worker_id: usize, reason: String) -> bool {
        let account_id = self.account_id(worker_id);
        self.update_account_capacity(
            account_id.as_deref(),
            worker_id,
            GatewayAccountCapacityStatus::Exhausted,
            Some(reason),
        )
    }

    pub fn mark_account_available_for_worker(&self, worker_id: usize) -> bool {
        let account_id = self.account_id(worker_id);
        self.update_account_capacity(
            account_id.as_deref(),
            worker_id,
            GatewayAccountCapacityStatus::Available,
            None,
        )
    }

    pub fn mark_unhealthy(&self, worker_id: usize, error: Option<String>) {
        let now = unix_timestamp_now();
        if let Some(worker) = write_guard(&self.workers).get_mut(worker_id) {
            let state_changed = worker.healthy || worker.reconnecting;
            worker.healthy = false;
            worker.reconnecting = false;
            worker.reconnect_attempt_count = 0;
            worker.last_error = error;
            if state_changed {
                worker.last_state_change_at = Some(now);
            }
            worker.last_error_at = worker.last_error.as_ref().map(|_| now);
            worker.next_reconnect_at = None;
        }
    }

    pub fn mark_reconnecting(
        &self,
        worker_id: usize,
        error: Option<String>,
        retry_delay: Duration,
    ) {
        let now = unix_timestamp_now();
        if let Some(worker) = write_guard(&self.workers).get_mut(worker_id) {
            let state_changed = worker.healthy || !worker.reconnecting;
            worker.reconnect_attempt_count = if worker.reconnecting {
                worker.reconnect_attempt_count.saturating_add(1)
            } else {
                0
            };
            worker.healthy = false;
            worker.reconnecting = true;
            worker.last_error = error;
            if state_changed {
                worker.last_state_change_at = Some(now);
            }
            worker.last_error_at = worker.last_error.as_ref().map(|_| now);
            worker.next_reconnect_at = Some(now.saturating_add(duration_ceil_seconds(retry_delay)));
        }
    }

    pub fn mark_healthy(&self, worker_id: usize) {
        let now = unix_timestamp_now();
        if let Some(worker) = write_guard(&self.workers).get_mut(worker_id) {
            let state_changed = !worker.healthy || worker.reconnecting;
            worker.healthy = true;
            worker.reconnecting = false;
            worker.reconnect_attempt_count = 0;
            if state_changed {
                worker.last_state_change_at = Some(now);
            }
            worker.next_reconnect_at = None;
        }
    }

    pub fn snapshot(&self) -> Vec<GatewayRemoteWorkerHealth> {
        let now = unix_timestamp_now();
        read_guard(&self.workers)
            .iter()
            .enumerate()
            .map(|(worker_id, worker)| GatewayRemoteWorkerHealth {
                worker_id,
                websocket_url: worker.websocket_url.clone(),
                account_id: worker.account_id.clone(),
                account_capacity: worker.account_capacity,
                account_capacity_reason: worker.account_capacity_reason.clone(),
                account_capacity_last_changed_at: worker.account_capacity_last_changed_at,
                healthy: worker.healthy,
                reconnecting: worker.reconnecting,
                reconnect_attempt_count: worker.reconnect_attempt_count,
                last_error: worker.last_error.clone(),
                last_state_change_at: worker.last_state_change_at,
                last_error_at: worker.last_error_at,
                next_reconnect_at: worker.next_reconnect_at,
                reconnect_backoff_remaining_seconds: worker
                    .next_reconnect_at
                    .map(|next_reconnect_at| next_reconnect_at.saturating_sub(now)),
            })
            .collect()
    }

    fn update_account_capacity(
        &self,
        account_id: Option<&str>,
        fallback_worker_id: usize,
        status: GatewayAccountCapacityStatus,
        reason: Option<String>,
    ) -> bool {
        let now = unix_timestamp_now();
        let mut any_state_changed = false;
        for (worker_id, worker) in write_guard(&self.workers).iter_mut().enumerate() {
            let same_account = match account_id {
                Some(account_id) => worker.account_id.as_deref() == Some(account_id),
                None => worker_id == fallback_worker_id,
            };
            if !same_account {
                continue;
            }

            let state_changed =
                worker.account_capacity != status || worker.account_capacity_reason != reason;
            worker.account_capacity = status;
            worker.account_capacity_reason = reason.clone();
            if state_changed {
                any_state_changed = true;
                worker.account_capacity_last_changed_at = Some(now);
            }
        }
        any_state_changed
    }
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn duration_ceil_seconds(duration: Duration) -> i64 {
    let whole_seconds = duration.as_secs();
    let needs_round_up = u64::from(duration.subsec_nanos() > 0);
    whole_seconds.saturating_add(needs_round_up) as i64
}

fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteWorkerHealthRegistry;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn workers_start_healthy_and_capture_last_error() {
        let registry = RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("acct-a".to_string()),
            ),
            ("ws://127.0.0.1:8082".to_string(), None),
        ]);

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(registry.is_healthy(1), true);

        registry.mark_unhealthy(1, Some("socket closed".to_string()));

        let snapshot = registry.snapshot();

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(registry.is_healthy(1), false);
        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot[0],
            crate::api::GatewayRemoteWorkerHealth {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:8081".to_string(),
                account_id: Some("acct-a".to_string()),
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
        assert_eq!(snapshot[1].worker_id, 1);
        assert_eq!(snapshot[1].websocket_url, "ws://127.0.0.1:8082");
        assert_eq!(snapshot[1].healthy, false);
        assert_eq!(snapshot[1].reconnecting, false);
        assert_eq!(snapshot[1].reconnect_attempt_count, 0);
        assert_eq!(snapshot[1].last_error.as_deref(), Some("socket closed"));
        assert_eq!(snapshot[1].last_state_change_at.is_some(), true);
        assert_eq!(snapshot[1].last_error_at.is_some(), true);
        assert_eq!(snapshot[1].next_reconnect_at, None);
        assert_eq!(snapshot[1].reconnect_backoff_remaining_seconds, None);
    }

    #[test]
    fn worker_account_labels_are_normalized() {
        let registry = RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("  acct-a  ".to_string()),
            ),
            ("ws://127.0.0.1:8082".to_string(), Some("   ".to_string())),
        ]);

        let snapshot = registry.snapshot();

        assert_eq!(registry.account_id(0), Some("acct-a".to_string()));
        assert_eq!(registry.account_id(1), None);
        assert_eq!(snapshot[0].account_id, Some("acct-a".to_string()));
        assert_eq!(snapshot[1].account_id, None);
    }

    #[test]
    fn workers_can_be_marked_healthy_again_without_losing_last_error() {
        let registry = RemoteWorkerHealthRegistry::new(vec!["ws://127.0.0.1:8081".to_string()]);

        registry.mark_unhealthy(0, Some("socket closed".to_string()));
        let unhealthy_snapshot = registry.snapshot();
        registry.mark_healthy(0);
        let healthy_snapshot = registry.snapshot();

        assert_eq!(registry.is_healthy(0), true);
        assert_eq!(unhealthy_snapshot.len(), 1);
        assert_eq!(healthy_snapshot.len(), 1);
        assert_eq!(unhealthy_snapshot[0].healthy, false);
        assert_eq!(unhealthy_snapshot[0].reconnecting, false);
        assert_eq!(unhealthy_snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(healthy_snapshot[0].healthy, true);
        assert_eq!(healthy_snapshot[0].reconnecting, false);
        assert_eq!(healthy_snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(
            healthy_snapshot[0].last_error,
            Some("socket closed".to_string())
        );
        assert_eq!(unhealthy_snapshot[0].last_state_change_at.is_some(), true);
        assert_eq!(healthy_snapshot[0].last_state_change_at.is_some(), true);
        assert_eq!(healthy_snapshot[0].last_error_at.is_some(), true);
        assert_eq!(healthy_snapshot[0].next_reconnect_at, None);
        assert_eq!(
            healthy_snapshot[0].reconnect_backoff_remaining_seconds,
            None
        );
    }

    #[test]
    fn unhealthy_worker_without_error_clears_last_error_timestamp() {
        let registry = RemoteWorkerHealthRegistry::new(vec!["ws://127.0.0.1:8081".to_string()]);

        registry.mark_unhealthy(0, Some("socket closed".to_string()));
        registry.mark_unhealthy(0, None);

        let snapshot = registry.snapshot();

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].healthy, false);
        assert_eq!(snapshot[0].reconnecting, false);
        assert_eq!(snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(snapshot[0].last_error, None);
        assert_eq!(snapshot[0].last_state_change_at.is_some(), true);
        assert_eq!(snapshot[0].last_error_at, None);
        assert_eq!(snapshot[0].next_reconnect_at, None);
        assert_eq!(snapshot[0].reconnect_backoff_remaining_seconds, None);
    }

    #[test]
    fn reconnecting_workers_expose_retry_state_and_clear_it_after_recovery() {
        let registry = RemoteWorkerHealthRegistry::new(vec!["ws://127.0.0.1:8081".to_string()]);

        registry.mark_reconnecting(
            0,
            Some("remote app server event stream ended".to_string()),
            Duration::from_millis(250),
        );
        let reconnecting_snapshot = registry.snapshot();
        registry.mark_healthy(0);
        let recovered_snapshot = registry.snapshot();

        assert_eq!(reconnecting_snapshot.len(), 1);
        assert_eq!(reconnecting_snapshot[0].healthy, false);
        assert_eq!(reconnecting_snapshot[0].reconnecting, true);
        assert_eq!(reconnecting_snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(
            reconnecting_snapshot[0].last_error.as_deref(),
            Some("remote app server event stream ended")
        );
        assert_eq!(
            reconnecting_snapshot[0].last_state_change_at.is_some(),
            true
        );
        assert_eq!(reconnecting_snapshot[0].last_error_at.is_some(), true);
        assert_eq!(reconnecting_snapshot[0].next_reconnect_at.is_some(), true);
        assert_eq!(
            reconnecting_snapshot[0]
                .reconnect_backoff_remaining_seconds
                .is_some(),
            true
        );

        assert_eq!(recovered_snapshot.len(), 1);
        assert_eq!(recovered_snapshot[0].healthy, true);
        assert_eq!(recovered_snapshot[0].reconnecting, false);
        assert_eq!(recovered_snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(recovered_snapshot[0].next_reconnect_at, None);
        assert_eq!(
            recovered_snapshot[0].reconnect_backoff_remaining_seconds,
            None
        );
    }

    #[test]
    fn repeated_reconnect_failures_keep_worker_in_reconnecting_state() {
        let registry = RemoteWorkerHealthRegistry::new(vec!["ws://127.0.0.1:8081".to_string()]);

        registry.mark_reconnecting(
            0,
            Some("first reconnect failure".to_string()),
            Duration::from_millis(250),
        );
        let first_snapshot = registry.snapshot();
        registry.mark_reconnecting(
            0,
            Some("second reconnect failure".to_string()),
            Duration::from_millis(250),
        );
        let second_snapshot = registry.snapshot();

        assert_eq!(first_snapshot.len(), 1);
        assert_eq!(second_snapshot.len(), 1);
        assert_eq!(first_snapshot[0].healthy, false);
        assert_eq!(first_snapshot[0].reconnecting, true);
        assert_eq!(first_snapshot[0].reconnect_attempt_count, 0);
        assert_eq!(second_snapshot[0].healthy, false);
        assert_eq!(second_snapshot[0].reconnecting, true);
        assert_eq!(second_snapshot[0].reconnect_attempt_count, 1);
        assert_eq!(
            second_snapshot[0].last_error.as_deref(),
            Some("second reconnect failure")
        );
        assert_eq!(
            second_snapshot[0].last_state_change_at,
            first_snapshot[0].last_state_change_at
        );
        assert_eq!(second_snapshot[0].next_reconnect_at.is_some(), true);
        assert_eq!(
            second_snapshot[0]
                .reconnect_backoff_remaining_seconds
                .is_some(),
            true
        );
    }

    #[test]
    fn account_capacity_updates_all_workers_with_the_same_account() {
        let registry = RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (
                "ws://127.0.0.1:8081".to_string(),
                Some("acct-a".to_string()),
            ),
            (
                "ws://127.0.0.1:8082".to_string(),
                Some("acct-a".to_string()),
            ),
            (
                "ws://127.0.0.1:8083".to_string(),
                Some("acct-b".to_string()),
            ),
        ]);

        registry.mark_account_exhausted_for_worker(0, "rate limit reached".to_string());

        assert_eq!(registry.account_has_capacity(0), false);
        assert_eq!(registry.account_has_capacity(1), false);
        assert_eq!(registry.account_has_capacity(2), true);
        assert_eq!(
            registry.snapshot()[0].account_capacity,
            crate::api::GatewayAccountCapacityStatus::Exhausted
        );

        registry.mark_account_available_for_worker(1);

        assert_eq!(registry.account_has_capacity(0), true);
        assert_eq!(registry.account_has_capacity(1), true);
    }
}
