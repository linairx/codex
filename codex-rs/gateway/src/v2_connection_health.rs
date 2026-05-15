use crate::api::GatewayV2AccountCapacityWorkerEventCounts;
use crate::api::GatewayV2ConnectionHealth;
use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayV2ConnectionPendingCounts {
    pub pending_client_request_count: usize,
    pub pending_server_request_count: usize,
    pub answered_but_unresolved_server_request_count: usize,
    pub server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GatewayV2ConnectionHealthState {
    next_connection_id: u64,
    active_connection_count: usize,
    active_connections: HashMap<u64, ActiveGatewayV2ConnectionHealth>,
    account_capacity_event_counts: BTreeMap<String, usize>,
    account_capacity_event_worker_counts: BTreeMap<usize, BTreeMap<String, usize>>,
    last_account_capacity_event: Option<String>,
    last_account_capacity_event_worker_id: Option<usize>,
    last_account_capacity_event_tenant_id: Option<String>,
    last_account_capacity_event_project_id: Option<String>,
    last_account_capacity_event_reason: Option<String>,
    last_account_capacity_event_at: Option<i64>,
    peak_active_connection_count: usize,
    total_connection_count: u64,
    last_connection_started_at: Option<i64>,
    last_connection_completed_at: Option<i64>,
    last_connection_duration_ms: Option<u64>,
    last_connection_outcome: Option<String>,
    last_connection_detail: Option<String>,
    last_connection_pending_client_request_count: usize,
    last_connection_pending_server_request_count: usize,
    last_connection_answered_but_unresolved_server_request_count: usize,
    last_connection_server_request_backlog_count: usize,
    last_connection_server_request_backlog_started_at: Option<i64>,
    last_connection_server_request_backlog_worker_counts:
        Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ActiveGatewayV2ConnectionHealth {
    pending_client_request_count: usize,
    pending_server_request_count: usize,
    answered_but_unresolved_server_request_count: usize,
    server_request_backlog_started_at: Option<i64>,
    server_request_backlog_worker_counts: Vec<GatewayV2ServerRequestBacklogWorkerCounts>,
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

    pub fn update_connection_pending_counts(
        &self,
        connection_id: u64,
        counts: GatewayV2ConnectionPendingCounts,
    ) {
        let mut state = write_guard(&self.state);
        if let Some(connection) = state.active_connections.get_mut(&connection_id) {
            connection.pending_client_request_count = counts.pending_client_request_count;
            let previous_server_request_backlog_count = connection
                .pending_server_request_count
                .saturating_add(connection.answered_but_unresolved_server_request_count);
            let server_request_backlog_count = counts
                .pending_server_request_count
                .saturating_add(counts.answered_but_unresolved_server_request_count);
            if previous_server_request_backlog_count == 0 && server_request_backlog_count > 0 {
                connection.server_request_backlog_started_at = Some(unix_timestamp_now());
            } else if server_request_backlog_count == 0 {
                connection.server_request_backlog_started_at = None;
            }
            connection.pending_server_request_count = counts.pending_server_request_count;
            connection.answered_but_unresolved_server_request_count =
                counts.answered_but_unresolved_server_request_count;
            connection.server_request_backlog_worker_counts =
                counts.server_request_backlog_worker_counts;
        }
    }

    pub fn record_account_capacity_event(
        &self,
        worker_id: usize,
        event: &str,
        tenant_id: Option<&str>,
        project_id: Option<&str>,
        reason: Option<&str>,
    ) {
        let mut state = write_guard(&self.state);
        *state
            .account_capacity_event_counts
            .entry(event.to_string())
            .or_insert(0) += 1;
        *state
            .account_capacity_event_worker_counts
            .entry(worker_id)
            .or_default()
            .entry(event.to_string())
            .or_insert(0) += 1;
        state.last_account_capacity_event = Some(event.to_string());
        state.last_account_capacity_event_worker_id = Some(worker_id);
        state.last_account_capacity_event_tenant_id = tenant_id.map(ToString::to_string);
        state.last_account_capacity_event_project_id = project_id.map(ToString::to_string);
        state.last_account_capacity_event_reason = reason.map(ToString::to_string);
        state.last_account_capacity_event_at = Some(unix_timestamp_now());
    }

    pub fn mark_connection_completed(
        &self,
        connection_id: u64,
        outcome: &str,
        detail: Option<&str>,
        duration: Duration,
        counts: GatewayV2ConnectionPendingCounts,
    ) {
        let mut state = write_guard(&self.state);
        let completed_connection = state.active_connections.remove(&connection_id);
        state.active_connection_count = state.active_connections.len();
        state.last_connection_completed_at = Some(unix_timestamp_now());
        state.last_connection_duration_ms =
            Some(duration.as_millis().min(u128::from(u64::MAX)) as u64);
        state.last_connection_outcome = Some(outcome.to_string());
        state.last_connection_detail = detail.map(ToString::to_string);
        state.last_connection_pending_client_request_count = counts.pending_client_request_count;
        state.last_connection_pending_server_request_count = counts.pending_server_request_count;
        state.last_connection_answered_but_unresolved_server_request_count =
            counts.answered_but_unresolved_server_request_count;
        state.last_connection_server_request_backlog_count = counts
            .pending_server_request_count
            .saturating_add(counts.answered_but_unresolved_server_request_count);
        state.last_connection_server_request_backlog_started_at = completed_connection
            .and_then(|connection| connection.server_request_backlog_started_at);
        state.last_connection_server_request_backlog_worker_counts =
            counts.server_request_backlog_worker_counts;
    }

    pub fn snapshot(&self) -> GatewayV2ConnectionHealth {
        let state = read_guard(&self.state);
        let active_connection_pending_client_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.pending_client_request_count)
            .sum();
        let active_connection_pending_server_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.pending_server_request_count)
            .sum();
        let active_connection_answered_but_unresolved_server_request_count: usize = state
            .active_connections
            .values()
            .map(|connection| connection.answered_but_unresolved_server_request_count)
            .sum();
        let active_connection_server_request_backlog_started_at = state
            .active_connections
            .values()
            .filter_map(|connection| connection.server_request_backlog_started_at)
            .min();
        let active_connection_server_request_backlog_count =
            active_connection_pending_server_request_count
                .saturating_add(active_connection_answered_but_unresolved_server_request_count);
        let active_connection_max_server_request_backlog_count = state
            .active_connections
            .values()
            .map(|connection| {
                connection
                    .pending_server_request_count
                    .saturating_add(connection.answered_but_unresolved_server_request_count)
            })
            .max()
            .unwrap_or(0);
        let mut active_connection_server_request_backlog_worker_counts = BTreeMap::new();
        for connection in state.active_connections.values() {
            for counts in &connection.server_request_backlog_worker_counts {
                let entry = active_connection_server_request_backlog_worker_counts
                    .entry(counts.worker_id)
                    .or_insert((0usize, 0usize));
                entry.0 = entry.0.saturating_add(counts.pending_server_request_count);
                entry.1 = entry
                    .1
                    .saturating_add(counts.answered_but_unresolved_server_request_count);
            }
        }
        let active_connection_server_request_backlog_worker_counts =
            active_connection_server_request_backlog_worker_counts
                .into_iter()
                .map(
                    |(worker_id, (pending_count, answered_but_unresolved_count))| {
                        GatewayV2ServerRequestBacklogWorkerCounts {
                            worker_id,
                            pending_server_request_count: pending_count,
                            answered_but_unresolved_server_request_count:
                                answered_but_unresolved_count,
                            server_request_backlog_count: pending_count
                                .saturating_add(answered_but_unresolved_count),
                        }
                    },
                )
                .collect();
        GatewayV2ConnectionHealth {
            active_connection_count: state.active_connection_count,
            active_connection_pending_client_request_count,
            active_connection_pending_server_request_count,
            active_connection_answered_but_unresolved_server_request_count,
            active_connection_server_request_backlog_count,
            active_connection_max_server_request_backlog_count,
            active_connection_server_request_backlog_started_at,
            active_connection_server_request_backlog_worker_counts,
            account_capacity_event_counts: state.account_capacity_event_counts.clone(),
            account_capacity_event_worker_counts: state
                .account_capacity_event_worker_counts
                .iter()
                .map(
                    |(worker_id, event_counts)| GatewayV2AccountCapacityWorkerEventCounts {
                        worker_id: *worker_id,
                        event_counts: event_counts.clone(),
                    },
                )
                .collect(),
            last_account_capacity_event: state.last_account_capacity_event.clone(),
            last_account_capacity_event_worker_id: state.last_account_capacity_event_worker_id,
            last_account_capacity_event_tenant_id: state
                .last_account_capacity_event_tenant_id
                .clone(),
            last_account_capacity_event_project_id: state
                .last_account_capacity_event_project_id
                .clone(),
            last_account_capacity_event_reason: state.last_account_capacity_event_reason.clone(),
            last_account_capacity_event_at: state.last_account_capacity_event_at,
            peak_active_connection_count: state.peak_active_connection_count,
            total_connection_count: state.total_connection_count,
            last_connection_started_at: state.last_connection_started_at,
            last_connection_completed_at: state.last_connection_completed_at,
            last_connection_duration_ms: state.last_connection_duration_ms,
            last_connection_outcome: state.last_connection_outcome.clone(),
            last_connection_detail: state.last_connection_detail.clone(),
            last_connection_pending_client_request_count: state
                .last_connection_pending_client_request_count,
            last_connection_pending_server_request_count: state
                .last_connection_pending_server_request_count,
            last_connection_answered_but_unresolved_server_request_count: state
                .last_connection_answered_but_unresolved_server_request_count,
            last_connection_server_request_backlog_count: state
                .last_connection_server_request_backlog_count,
            last_connection_server_request_backlog_started_at: state
                .last_connection_server_request_backlog_started_at,
            last_connection_server_request_backlog_worker_counts: state
                .last_connection_server_request_backlog_worker_counts
                .clone(),
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
    use super::GatewayV2ConnectionPendingCounts;
    use crate::api::GatewayV2ConnectionHealth;
    use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[test]
    fn snapshot_starts_empty() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        assert_eq!(
            registry.snapshot(),
            GatewayV2ConnectionHealth {
                active_connection_count: 0,
                active_connection_pending_client_request_count: 0,
                active_connection_pending_server_request_count: 0,
                active_connection_answered_but_unresolved_server_request_count: 0,
                active_connection_server_request_backlog_count: 0,
                active_connection_max_server_request_backlog_count: 0,
                active_connection_server_request_backlog_started_at: None,
                active_connection_server_request_backlog_worker_counts: Vec::new(),
                account_capacity_event_counts: BTreeMap::new(),
                account_capacity_event_worker_counts: Vec::new(),
                last_account_capacity_event: None,
                last_account_capacity_event_worker_id: None,
                last_account_capacity_event_tenant_id: None,
                last_account_capacity_event_project_id: None,
                last_account_capacity_event_reason: None,
                last_account_capacity_event_at: None,
                peak_active_connection_count: 0,
                total_connection_count: 0,
                last_connection_started_at: None,
                last_connection_completed_at: None,
                last_connection_duration_ms: None,
                last_connection_outcome: None,
                last_connection_detail: None,
                last_connection_pending_client_request_count: 0,
                last_connection_pending_server_request_count: 0,
                last_connection_answered_but_unresolved_server_request_count: 0,
                last_connection_server_request_backlog_count: 0,
                last_connection_server_request_backlog_started_at: None,
                last_connection_server_request_backlog_worker_counts: Vec::new(),
            }
        );
    }

    #[test]
    fn snapshot_tracks_active_and_last_completed_connection() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        let first_connection_id = registry.mark_connection_started();
        let second_connection_id = registry.mark_connection_started();
        registry.update_connection_pending_counts(
            first_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 7,
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(1),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
            },
        );
        registry.update_connection_pending_counts(
            second_connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 11,
                pending_server_request_count: 5,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: None,
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 0,
                        server_request_backlog_count: 1,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 4,
                        answered_but_unresolved_server_request_count: 4,
                        server_request_backlog_count: 8,
                    },
                ],
            },
        );
        let active_snapshot = registry.snapshot();
        assert_eq!(active_snapshot.active_connection_count, 2);
        assert_eq!(
            active_snapshot.active_connection_pending_client_request_count,
            18
        );
        assert_eq!(
            active_snapshot.active_connection_pending_server_request_count,
            8
        );
        assert_eq!(
            active_snapshot.active_connection_answered_but_unresolved_server_request_count,
            6
        );
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_count,
            14
        );
        assert_eq!(
            active_snapshot.active_connection_max_server_request_backlog_count,
            9
        );
        assert_eq!(
            active_snapshot.active_connection_server_request_backlog_worker_counts,
            vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: None,
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 0,
                    server_request_backlog_count: 1,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(1),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 3,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(2),
                    pending_server_request_count: 5,
                    answered_but_unresolved_server_request_count: 5,
                    server_request_backlog_count: 10,
                },
            ]
        );
        assert_eq!(
            active_snapshot
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
        assert_eq!(
            active_snapshot.account_capacity_event_counts,
            BTreeMap::new()
        );
        assert_eq!(active_snapshot.account_capacity_event_worker_counts, vec![]);
        assert_eq!(active_snapshot.last_account_capacity_event, None);
        assert_eq!(active_snapshot.last_account_capacity_event_worker_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_tenant_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_project_id, None);
        assert_eq!(active_snapshot.last_account_capacity_event_reason, None);
        assert_eq!(active_snapshot.last_account_capacity_event_at, None);
        assert_eq!(active_snapshot.peak_active_connection_count, 2);
        assert_eq!(active_snapshot.total_connection_count, 2);
        assert_eq!(active_snapshot.last_connection_started_at.is_some(), true);

        registry.mark_connection_completed(
            first_connection_id,
            "client_disconnected",
            Some("socket closed"),
            Duration::from_millis(42),
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 7,
                pending_server_request_count: 3,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_worker_counts: vec![
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(1),
                        pending_server_request_count: 2,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 3,
                    },
                    GatewayV2ServerRequestBacklogWorkerCounts {
                        worker_id: Some(2),
                        pending_server_request_count: 1,
                        answered_but_unresolved_server_request_count: 1,
                        server_request_backlog_count: 2,
                    },
                ],
            },
        );

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.active_connection_count, 1);
        assert_eq!(snapshot.active_connection_pending_client_request_count, 11);
        assert_eq!(snapshot.active_connection_pending_server_request_count, 5);
        assert_eq!(
            snapshot.active_connection_answered_but_unresolved_server_request_count,
            4
        );
        assert_eq!(snapshot.active_connection_server_request_backlog_count, 9);
        assert_eq!(
            snapshot.active_connection_max_server_request_backlog_count,
            9
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
        assert_eq!(snapshot.last_connection_pending_client_request_count, 7);
        assert_eq!(snapshot.last_connection_pending_server_request_count, 3);
        assert_eq!(
            snapshot.last_connection_answered_but_unresolved_server_request_count,
            2
        );
        assert_eq!(snapshot.last_connection_server_request_backlog_count, 5);
        assert_eq!(
            snapshot.last_connection_server_request_backlog_worker_counts,
            vec![
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(1),
                    pending_server_request_count: 2,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 3,
                },
                GatewayV2ServerRequestBacklogWorkerCounts {
                    worker_id: Some(2),
                    pending_server_request_count: 1,
                    answered_but_unresolved_server_request_count: 1,
                    server_request_backlog_count: 2,
                },
            ]
        );
        assert_eq!(
            snapshot
                .last_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
        assert_eq!(snapshot.last_connection_started_at.is_some(), true);
        assert_eq!(snapshot.last_connection_completed_at.is_some(), true);
    }

    #[test]
    fn server_request_backlog_timestamp_resets_when_backlog_clears() {
        let registry = GatewayV2ConnectionHealthRegistry::default();
        let connection_id = registry.mark_connection_started();

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_worker_counts: Vec::new(),
            },
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts::default(),
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at,
            None
        );

        registry.update_connection_pending_counts(
            connection_id,
            GatewayV2ConnectionPendingCounts {
                pending_client_request_count: 0,
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_worker_counts: Vec::new(),
            },
        );
        assert_eq!(
            registry
                .snapshot()
                .active_connection_server_request_backlog_started_at
                .is_some(),
            true
        );
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
            GatewayV2ConnectionPendingCounts::default(),
        );

        assert_eq!(
            registry.snapshot().last_connection_duration_ms,
            Some(u64::MAX)
        );
    }

    #[test]
    fn snapshot_tracks_account_capacity_events() {
        let registry = GatewayV2ConnectionHealthRegistry::default();

        registry.record_account_capacity_event(
            1,
            "exhausted",
            Some("tenant-a"),
            Some("project-a"),
            Some("quota exhausted"),
        );
        registry.record_account_capacity_event(
            2,
            "thread_read_handoff_success",
            Some("tenant-a"),
            Some("project-a"),
            Some("restored"),
        );
        registry.record_account_capacity_event(
            3,
            "exhausted",
            Some("tenant-b"),
            None,
            Some("billing limit"),
        );

        let snapshot = registry.snapshot();
        assert_eq!(
            snapshot.account_capacity_event_counts,
            BTreeMap::from([
                ("exhausted".to_string(), 2),
                ("thread_read_handoff_success".to_string(), 1),
            ])
        );
        assert_eq!(
            snapshot.account_capacity_event_worker_counts,
            vec![
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 1,
                    event_counts: BTreeMap::from([("exhausted".to_string(), 1)]),
                },
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 2,
                    event_counts: BTreeMap::from([("thread_read_handoff_success".to_string(), 1)]),
                },
                crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                    worker_id: 3,
                    event_counts: BTreeMap::from([("exhausted".to_string(), 1)]),
                },
            ]
        );
        assert_eq!(
            snapshot.last_account_capacity_event,
            Some("exhausted".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_worker_id, Some(3));
        assert_eq!(
            snapshot.last_account_capacity_event_tenant_id,
            Some("tenant-b".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_project_id, None);
        assert_eq!(
            snapshot.last_account_capacity_event_reason,
            Some("billing limit".to_string())
        );
        assert_eq!(snapshot.last_account_capacity_event_at.is_some(), true);
    }
}
