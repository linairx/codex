use crate::api::GatewayV2ConnectionHealth;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GatewayV2ConnectionHealthState {
    active_connection_count: usize,
    peak_active_connection_count: usize,
    total_connection_count: u64,
    last_connection_started_at: Option<i64>,
    last_connection_completed_at: Option<i64>,
    last_connection_outcome: Option<String>,
    last_connection_detail: Option<String>,
    last_connection_pending_server_request_count: usize,
    last_connection_answered_but_unresolved_server_request_count: usize,
}

#[derive(Debug, Default)]
pub struct GatewayV2ConnectionHealthRegistry {
    state: RwLock<GatewayV2ConnectionHealthState>,
}

impl GatewayV2ConnectionHealthRegistry {
    pub fn mark_connection_started(&self) {
        let mut state = write_guard(&self.state);
        state.active_connection_count = state.active_connection_count.saturating_add(1);
        state.peak_active_connection_count = state
            .peak_active_connection_count
            .max(state.active_connection_count);
        state.total_connection_count = state.total_connection_count.saturating_add(1);
        state.last_connection_started_at = Some(unix_timestamp_now());
    }

    pub fn mark_connection_completed(
        &self,
        outcome: &str,
        detail: Option<&str>,
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
    ) {
        let mut state = write_guard(&self.state);
        state.active_connection_count = state.active_connection_count.saturating_sub(1);
        state.last_connection_completed_at = Some(unix_timestamp_now());
        state.last_connection_outcome = Some(outcome.to_string());
        state.last_connection_detail = detail.map(ToString::to_string);
        state.last_connection_pending_server_request_count = pending_server_request_count;
        state.last_connection_answered_but_unresolved_server_request_count =
            answered_but_unresolved_server_request_count;
    }

    pub fn snapshot(&self) -> GatewayV2ConnectionHealth {
        let state = read_guard(&self.state);
        GatewayV2ConnectionHealth {
            active_connection_count: state.active_connection_count,
            peak_active_connection_count: state.peak_active_connection_count,
            total_connection_count: state.total_connection_count,
            last_connection_started_at: state.last_connection_started_at,
            last_connection_completed_at: state.last_connection_completed_at,
            last_connection_outcome: state.last_connection_outcome.clone(),
            last_connection_detail: state.last_connection_detail.clone(),
            last_connection_pending_server_request_count: state
                .last_connection_pending_server_request_count,
            last_connection_answered_but_unresolved_server_request_count: state
                .last_connection_answered_but_unresolved_server_request_count,
        }
    }
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
    use super::GatewayV2ConnectionHealthRegistry;
    use crate::api::GatewayV2ConnectionHealth;
    use pretty_assertions::assert_eq;

    #[test]
    fn snapshot_starts_empty() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        assert_eq!(
            registry.snapshot(),
            GatewayV2ConnectionHealth {
                active_connection_count: 0,
                peak_active_connection_count: 0,
                total_connection_count: 0,
                last_connection_started_at: None,
                last_connection_completed_at: None,
                last_connection_outcome: None,
                last_connection_detail: None,
                last_connection_pending_server_request_count: 0,
                last_connection_answered_but_unresolved_server_request_count: 0,
            }
        );
    }

    #[test]
    fn snapshot_tracks_active_and_last_completed_connection() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.mark_connection_started();
        registry.mark_connection_started();
        let active_snapshot = registry.snapshot();
        assert_eq!(active_snapshot.active_connection_count, 2);
        assert_eq!(active_snapshot.peak_active_connection_count, 2);
        assert_eq!(active_snapshot.total_connection_count, 2);
        assert_eq!(active_snapshot.last_connection_started_at.is_some(), true);

        registry.mark_connection_completed("client_disconnected", Some("socket closed"), 3, 2);

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_count, 1);
        assert_eq!(snapshot.peak_active_connection_count, 2);
        assert_eq!(snapshot.total_connection_count, 2);
        assert_eq!(
            snapshot.last_connection_outcome,
            Some("client_disconnected".to_string())
        );
        assert_eq!(
            snapshot.last_connection_detail,
            Some("socket closed".to_string())
        );
        assert_eq!(snapshot.last_connection_pending_server_request_count, 3);
        assert_eq!(
            snapshot.last_connection_answered_but_unresolved_server_request_count,
            2
        );
        assert_eq!(snapshot.last_connection_started_at.is_some(), true);
        assert_eq!(snapshot.last_connection_completed_at.is_some(), true);
    }
}
