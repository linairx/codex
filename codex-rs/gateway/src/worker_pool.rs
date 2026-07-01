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
        let mut accounts = Vec::new();
        let mut worker_slots = Vec::with_capacity(remote_runtime.workers.len());

        for (worker_id, worker) in remote_runtime.workers.iter().enumerate() {
            let account_id = normalize_remote_account_id(worker.account_id.clone());
            if let Some(account_id) = account_id.clone() {
                accounts.push(GatewayAccountPoolEntry {
                    account_id,
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
                account_login_state_path: None,
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
                account.policy_eligible = false;
                account.policy_ineligibility_reason = Some("leased worker is missing".to_string());
                continue;
            };

            account.account_capacity = worker.account_capacity;
            account.account_capacity_reason = worker.account_capacity_reason.clone();
            if !worker.healthy {
                account.policy_eligible = false;
                account.policy_ineligibility_reason =
                    Some("leased worker is unhealthy".to_string());
            } else if worker.account_capacity == GatewayAccountCapacityStatus::Exhausted {
                account.policy_eligible = false;
                account.policy_ineligibility_reason = Some(
                    worker
                        .account_capacity_reason
                        .clone()
                        .unwrap_or_else(|| "account capacity is exhausted".to_string()),
                );
            } else {
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
    GatewayWorkerPoolSnapshot {
        account_count: accounts.len(),
        leased_account_count: accounts
            .iter()
            .filter(|account| account.lease_state == GatewayAccountLeaseState::Leased)
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
        accounts,
        worker_slots,
    }
}

fn refresh_snapshot_counts(snapshot: &mut GatewayWorkerPoolSnapshot) {
    snapshot.account_count = snapshot.accounts.len();
    snapshot.leased_account_count = snapshot
        .accounts
        .iter()
        .filter(|account| account.lease_state == GatewayAccountLeaseState::Leased)
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
