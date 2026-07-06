use crate::api::GatewayAccountCapacityStatus;
use crate::api::GatewayAccountLeaseState;
use crate::api::GatewayAccountPoolEntry;
use crate::api::GatewayProjectWorkerRoute;
use crate::api::GatewayWorkerPoolSlot;
use crate::api::GatewayWorkerPoolSnapshot;
use crate::config::GatewayRemoteRuntimeConfig;
use crate::config::normalize_remote_account_id;
use crate::remote_health::RemoteWorkerHealthRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayWorkerPoolState {
    accounts: Vec<GatewayAccountPoolEntry>,
    worker_slots: Vec<GatewayWorkerPoolSlot>,
}

impl GatewayWorkerPoolState {
    pub fn from_remote_runtime(remote_runtime: &GatewayRemoteRuntimeConfig) -> Self {
        Self::from_remote_runtime_with_account_pool(remote_runtime, &[], &[], &[])
    }

    pub fn from_remote_runtime_with_account_login_state_paths(
        remote_runtime: &GatewayRemoteRuntimeConfig,
        account_login_state_paths: &[Option<String>],
    ) -> Self {
        Self::from_remote_runtime_with_account_pool(
            remote_runtime,
            &[],
            account_login_state_paths,
            &[],
        )
    }

    pub fn from_remote_runtime_with_account_pool(
        remote_runtime: &GatewayRemoteRuntimeConfig,
        account_pool_account_ids: &[String],
        account_login_state_paths: &[Option<String>],
        account_pool_login_state_paths: &[Option<String>],
    ) -> Self {
        let mut accounts = Vec::new();
        let mut worker_slots = Vec::with_capacity(remote_runtime.workers.len());

        for (worker_id, worker) in remote_runtime.workers.iter().enumerate() {
            let account_id = normalize_remote_account_id(worker.account_id.clone());
            let worker_login_state_path =
                account_login_state_paths.get(worker_id).cloned().flatten();
            let pool_login_state_path = account_id.as_deref().and_then(|account_id| {
                find_account_pool_login_state_path(
                    account_id,
                    account_pool_account_ids,
                    account_pool_login_state_paths,
                )
            });
            if let Some(account_id) = account_id.clone() {
                accounts.push(GatewayAccountPoolEntry {
                    account_id,
                    account_login_state_path: worker_login_state_path
                        .clone()
                        .or(pool_login_state_path),
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(worker_id),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                });
            }

            worker_slots.push(GatewayWorkerPoolSlot {
                worker_id,
                websocket_url: worker.websocket_url.clone(),
                account_id,
                account_login_state_path: worker_login_state_path,
                account_capacity: None,
                account_capacity_reason: None,
                healthy: None,
                reconnecting: None,
                last_error: None,
            });
        }

        for (index, account_id) in account_pool_account_ids.iter().enumerate() {
            let Some(account_id) = normalize_remote_account_id(Some(account_id.clone())) else {
                continue;
            };
            if accounts
                .iter()
                .any(|account| account.account_id == account_id)
            {
                continue;
            }
            accounts.push(GatewayAccountPoolEntry {
                account_id,
                account_login_state_path: account_pool_login_state_paths
                    .get(index)
                    .cloned()
                    .flatten(),
                lease_state: GatewayAccountLeaseState::Available,
                leased_worker_id: None,
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: false,
                policy_ineligibility_reason: Some("account is not leased to a worker".to_string()),
                cooldown_reason: None,
                last_error: None,
            });
        }

        Self {
            accounts,
            worker_slots,
        }
    }

    pub fn snapshot(&self) -> GatewayWorkerPoolSnapshot {
        snapshot_from_parts(self.accounts.clone(), self.worker_slots.clone())
    }

    pub fn snapshot_with_worker_health(
        &self,
        worker_health: &RemoteWorkerHealthRegistry,
    ) -> GatewayWorkerPoolSnapshot {
        let remote_workers = worker_health.snapshot();
        let mut snapshot = self.snapshot();
        for worker_slot in &mut snapshot.worker_slots {
            if let Some(worker) = remote_workers
                .iter()
                .find(|worker| worker.worker_id == worker_slot.worker_id)
            {
                worker_slot.healthy = Some(worker.healthy);
                worker_slot.reconnecting = Some(worker.reconnecting);
                worker_slot.last_error = worker.last_error.clone();
                if worker_slot.account_id.is_some() {
                    worker_slot.account_capacity = Some(worker.account_capacity);
                    worker_slot.account_capacity_reason = worker.account_capacity_reason.clone();
                } else {
                    worker_slot.account_capacity = None;
                    worker_slot.account_capacity_reason = None;
                }
            } else {
                worker_slot.healthy = Some(false);
                worker_slot.reconnecting = Some(false);
                worker_slot.last_error =
                    Some("worker slot is missing from health registry".to_string());
                worker_slot.account_capacity = None;
                worker_slot.account_capacity_reason = None;
            }
        }
        for account in &mut snapshot.accounts {
            let Some(worker_id) = account.leased_worker_id else {
                continue;
            };
            let Some(worker) = remote_workers
                .iter()
                .find(|worker| worker.worker_id == worker_id)
            else {
                account.account_capacity = GatewayAccountCapacityStatus::Exhausted;
                account.account_capacity_reason = Some("leased worker is missing".to_string());
                account.last_error = Some("leased worker is missing".to_string());
                account.policy_eligible = false;
                account.policy_ineligibility_reason = Some("leased worker is missing".to_string());
                continue;
            };

            account.account_capacity = worker.account_capacity;
            account.account_capacity_reason = worker.account_capacity_reason.clone();
            account.last_error = worker.last_error.clone();
            if !worker.healthy {
                account.lease_state = GatewayAccountLeaseState::Leased;
                account.cooldown_reason = None;
                account.policy_eligible = false;
                account.policy_ineligibility_reason =
                    Some("leased worker is unhealthy".to_string());
            } else if worker.account_capacity == GatewayAccountCapacityStatus::Exhausted {
                let exhausted_reason = worker
                    .account_capacity_reason
                    .clone()
                    .unwrap_or_else(|| "account capacity is exhausted".to_string());
                account.lease_state = GatewayAccountLeaseState::Cooldown;
                account.cooldown_reason = Some(exhausted_reason.clone());
                account.policy_eligible = false;
                account.policy_ineligibility_reason = Some(exhausted_reason);
            } else {
                account.lease_state = GatewayAccountLeaseState::Leased;
                account.cooldown_reason = None;
                account.policy_eligible = true;
                account.policy_ineligibility_reason = None;
            }
        }
        refresh_snapshot_counts(&mut snapshot);
        snapshot
    }

    pub fn snapshot_with_worker_health_and_project_routes(
        &self,
        worker_health: &RemoteWorkerHealthRegistry,
        project_worker_routes: &[GatewayProjectWorkerRoute],
    ) -> GatewayWorkerPoolSnapshot {
        let mut snapshot = self.snapshot_with_worker_health(worker_health);
        apply_project_route_counts(&mut snapshot, project_worker_routes);
        refresh_snapshot_counts(&mut snapshot);
        snapshot
    }
}

fn snapshot_from_parts(
    accounts: Vec<GatewayAccountPoolEntry>,
    worker_slots: Vec<GatewayWorkerPoolSlot>,
) -> GatewayWorkerPoolSnapshot {
    let (worker_account_login_state_path_count, pool_account_login_state_path_count) =
        account_login_state_path_source_counts(&accounts, &worker_slots);
    GatewayWorkerPoolSnapshot {
        account_count: accounts.len(),
        available_account_count: accounts
            .iter()
            .filter(|account| account.lease_state == GatewayAccountLeaseState::Available)
            .count(),
        leased_account_count: accounts
            .iter()
            .filter(|account| account.lease_state == GatewayAccountLeaseState::Leased)
            .count(),
        cooldown_account_count: accounts
            .iter()
            .filter(|account| account.lease_state == GatewayAccountLeaseState::Cooldown)
            .count(),
        policy_eligible_account_count: accounts
            .iter()
            .filter(|account| account.policy_eligible)
            .count(),
        policy_ineligible_account_count: accounts
            .iter()
            .filter(|account| !account.policy_eligible)
            .count(),
        worker_slot_count: worker_slots.len(),
        bound_worker_slot_count: worker_slots
            .iter()
            .filter(|worker_slot| worker_slot.account_id.is_some())
            .count(),
        healthy_worker_slot_count: worker_slots
            .iter()
            .filter(|worker_slot| worker_slot.healthy == Some(true))
            .count(),
        unhealthy_worker_slot_count: worker_slots
            .iter()
            .filter(|worker_slot| worker_slot.healthy == Some(false))
            .count(),
        reconnecting_worker_slot_count: worker_slots
            .iter()
            .filter(|worker_slot| worker_slot.reconnecting == Some(true))
            .count(),
        account_login_state_path_count: account_login_state_path_count(&accounts, &worker_slots),
        worker_account_login_state_path_count,
        pool_account_login_state_path_count,
        accounts,
        worker_slots,
    }
}

fn refresh_snapshot_counts(snapshot: &mut GatewayWorkerPoolSnapshot) {
    snapshot.account_count = snapshot.accounts.len();
    snapshot.available_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| account.lease_state == GatewayAccountLeaseState::Available)
        .count();
    snapshot.leased_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| account.lease_state == GatewayAccountLeaseState::Leased)
        .count();
    snapshot.cooldown_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| account.lease_state == GatewayAccountLeaseState::Cooldown)
        .count();
    snapshot.policy_eligible_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| account.policy_eligible)
        .count();
    snapshot.policy_ineligible_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| !account.policy_eligible)
        .count();
    snapshot.worker_slot_count = snapshot.worker_slots.len();
    snapshot.bound_worker_slot_count = snapshot
        .worker_slots
        .iter()
        .filter(|worker_slot| worker_slot.account_id.is_some())
        .count();
    snapshot.healthy_worker_slot_count = snapshot
        .worker_slots
        .iter()
        .filter(|worker_slot| worker_slot.healthy == Some(true))
        .count();
    snapshot.unhealthy_worker_slot_count = snapshot
        .worker_slots
        .iter()
        .filter(|worker_slot| worker_slot.healthy == Some(false))
        .count();
    snapshot.reconnecting_worker_slot_count = snapshot
        .worker_slots
        .iter()
        .filter(|worker_slot| worker_slot.reconnecting == Some(true))
        .count();
    snapshot.account_login_state_path_count =
        account_login_state_path_count(&snapshot.accounts, &snapshot.worker_slots);
    (
        snapshot.worker_account_login_state_path_count,
        snapshot.pool_account_login_state_path_count,
    ) = account_login_state_path_source_counts(&snapshot.accounts, &snapshot.worker_slots);
}

fn find_account_pool_login_state_path(
    account_id: &str,
    account_pool_account_ids: &[String],
    account_pool_login_state_paths: &[Option<String>],
) -> Option<String> {
    account_pool_account_ids
        .iter()
        .enumerate()
        .find_map(|(index, pool_account_id)| {
            (normalize_remote_account_id(Some(pool_account_id.clone())).as_deref()
                == Some(account_id))
            .then(|| account_pool_login_state_paths.get(index).cloned().flatten())
            .flatten()
        })
}

fn account_login_state_path_count(
    accounts: &[GatewayAccountPoolEntry],
    worker_slots: &[GatewayWorkerPoolSlot],
) -> usize {
    accounts
        .iter()
        .filter(|account| account.account_login_state_path.is_some())
        .count()
        + worker_slots
            .iter()
            .filter(|worker_slot| {
                worker_slot.account_id.is_none() && worker_slot.account_login_state_path.is_some()
            })
            .count()
}

fn account_login_state_path_source_counts(
    accounts: &[GatewayAccountPoolEntry],
    worker_slots: &[GatewayWorkerPoolSlot],
) -> (usize, usize) {
    let mut worker_path_count = worker_slots
        .iter()
        .filter(|worker_slot| {
            worker_slot.account_id.is_none() && worker_slot.account_login_state_path.is_some()
        })
        .count();
    let mut pool_path_count = 0;

    for account in accounts {
        let Some(account_login_state_path) = account.account_login_state_path.as_deref() else {
            continue;
        };
        let worker_path_matches = account.leased_worker_id.is_some_and(|leased_worker_id| {
            worker_slots
                .iter()
                .find(|worker_slot| worker_slot.worker_id == leased_worker_id)
                .and_then(|worker_slot| worker_slot.account_login_state_path.as_deref())
                == Some(account_login_state_path)
        });
        if worker_path_matches {
            worker_path_count += 1;
        } else {
            pool_path_count += 1;
        }
    }

    (worker_path_count, pool_path_count)
}

fn apply_project_route_counts(
    snapshot: &mut GatewayWorkerPoolSnapshot,
    project_worker_routes: &[GatewayProjectWorkerRoute],
) {
    for account in &mut snapshot.accounts {
        let Some(leased_worker_id) = account.leased_worker_id else {
            continue;
        };
        account.project_route_count = project_worker_routes
            .iter()
            .filter(|route| route.worker_id == leased_worker_id)
            .count();
    }
}

#[cfg(test)]
#[path = "worker_pool_tests.rs"]
mod tests;
