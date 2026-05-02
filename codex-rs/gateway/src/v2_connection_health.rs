use crate::api::GatewayV2ConnectionHealth;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GatewayV2ConnectionHealthState {
    next_connection_id: u64,
    active_connection_count: usize,
    active_connections: HashMap<u64, ActiveGatewayV2ConnectionHealth>,
    peak_active_connection_count: usize,
    total_connection_count: u64,
    last_connection_started_at: Option<i64>,
    last_connection_completed_at: Option<i64>,
    last_connection_duration_ms: Option<u64>,
    last_connection_outcome: Option<String>,
    last_connection_detail: Option<String>,
    last_connection_pending_server_request_count: usize,
    last_connection_answered_but_unresolved_server_request_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ActiveGatewayV2ConnectionHealth {
    pending_server_request_count: usize,
    answered_but_unresolved_server_request_count: usize,
}

#[derive(Debug, Default)]
pub struct GatewayV2ConnectionHealthRegistry {
    state: RwLock<GatewayV2ConnectionHealthState>,
}

impl GatewayV2ConnectionHealthRegistry {
    pub fn mark_connection_started(&self) -> u64 {
        let mut state = write_guard(&self.state);
        let connection_id = state.next_connection_id;
        state.next_connection_id = state.next_connection_id.saturating_add(1);
        state
            .active_connections
            .insert(connection_id, ActiveGatewayV2ConnectionHealth::default());
        state.active_connection_count = state.active_connections.len();
        state.peak_active_connection_count = state
            .peak_active_connection_count
            .max(state.active_connection_count);
        state.total_connection_count = state.total_connection_count.saturating_add(1);
        state.last_connection_started_at = Some(unix_timestamp_now());
        connection_id
    }

    pub fn update_connection_server_request_counts(
        &self,
        connection_id: u64,
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
    ) {
        let mut state = write_guard(&self.state);
        if let Some(connection) = state.active_connections.get_mut(&connection_id) {
            connection.pending_server_request_count = pending_server_request_count;
            connection.answered_but_unresolved_server_request_count =
                answered_but_unresolved_server_request_count;
        }
    }

    pub fn mark_connection_completed(
        &self,
        connection_id: u64,
        outcome: &str,
        detail: Option<&str>,
        duration: Duration,
        pending_server_request_count: usize,
        answered_but_unresolved_server_request_count: usize,
    ) {
        let mut state = write_guard(&self.state);
        state.active_connections.remove(&connection_id);
        state.active_connection_count = state.active_connections.len();
        state.last_connection_completed_at = Some(unix_timestamp_now());
        state.last_connection_duration_ms =
            Some(duration.as_millis().min(u128::from(u64::MAX)) as u64);
        state.last_connection_outcome = Some(outcome.to_string());
        state.last_connection_detail = detail.map(ToString::to_string);
        state.last_connection_pending_server_request_count = pending_server_request_count;
        state.last_connection_answered_but_unresolved_server_request_count =
            answered_but_unresolved_server_request_count;
    }

    pub fn snapshot(&self) -> GatewayV2ConnectionHealth {
        let state = read_guard(&self.state);
        let active_connection_pending_server_request_count = state
            .active_connections
            .values()
            .map(|connection| connection.pending_server_request_count)
            .sum();
        let active_connection_answered_but_unresolved_server_request_count = state
            .active_connections
            .values()
            .map(|connection| connection.answered_but_unresolved_server_request_count)
            .sum();
        GatewayV2ConnectionHealth {
            active_connection_count: state.active_connection_count,
            active_connection_pending_server_request_count,
            active_connection_answered_but_unresolved_server_request_count,
            peak_active_connection_count: state.peak_active_connection_count,
            total_connection_count: state.total_connection_count,
            last_connection_started_at: state.last_connection_started_at,
            last_connection_completed_at: state.last_connection_completed_at,
            last_connection_duration_ms: state.last_connection_duration_ms,
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
    use std::time::Duration;

    #[test]
    fn snapshot_starts_empty() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        assert_eq!(
            registry.snapshot(),
            GatewayV2ConnectionHealth {
                active_connection_count: 0,
                active_connection_pending_server_request_count: 0,
                active_connection_answered_but_unresolved_server_request_count: 0,
                peak_active_connection_count: 0,
                total_connection_count: 0,
                last_connection_started_at: None,
                last_connection_completed_at: None,
                last_connection_duration_ms: None,
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

        let first_connection_id = registry.mark_connection_started();
        let second_connection_id = registry.mark_connection_started();
        registry.update_connection_server_request_counts(first_connection_id, 3, 2);
        registry.update_connection_server_request_counts(second_connection_id, 5, 4);
        let active_snapshot = registry.snapshot();
        assert_eq!(active_snapshot.active_connection_count, 2);
        assert_eq!(
            active_snapshot.active_connection_pending_server_request_count,
            8
        );
        assert_eq!(
            active_snapshot.active_connection_answered_but_unresolved_server_request_count,
            6
        );
        assert_eq!(active_snapshot.peak_active_connection_count, 2);
        assert_eq!(active_snapshot.total_connection_count, 2);
        assert_eq!(active_snapshot.last_connection_started_at.is_some(), true);

        registry.mark_connection_completed(
            first_connection_id,
            "client_disconnected",
            Some("socket closed"),
            Duration::from_millis(42),
            3,
            2,
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_count, 1);
        assert_eq!(snapshot.active_connection_pending_server_request_count, 5);
        assert_eq!(
            snapshot.active_connection_answered_but_unresolved_server_request_count,
            4
        );
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
        assert_eq!(snapshot.last_connection_duration_ms, Some(42));
        assert_eq!(snapshot.last_connection_pending_server_request_count, 3);
        assert_eq!(
            snapshot.last_connection_answered_but_unresolved_server_request_count,
            2
        );
        assert_eq!(snapshot.last_connection_started_at.is_some(), true);
        assert_eq!(snapshot.last_connection_completed_at.is_some(), true);
    }

    #[test]
    fn snapshot_clamps_last_completed_connection_duration() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        let connection_id = registry.mark_connection_started();
        registry.mark_connection_completed(
            connection_id,
            "client_disconnected",
            None,
            Duration::from_secs(u64::MAX),
            0,
            0,
        );

        assert_eq!(
            registry.snapshot().last_connection_duration_ms,
            Some(u64::MAX)
        );
    }
}
