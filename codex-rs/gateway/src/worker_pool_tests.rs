use super::*;
use crate::config::GatewayRemoteSelectionPolicy;
use crate::config::GatewayRemoteWorkerConfig;
use pretty_assertions::assert_eq;

#[test]
fn static_remote_runtime_builds_leased_accounts_and_worker_slots() {
    let pool = GatewayWorkerPoolState::from_remote_runtime(&GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                auth_token: None,
                account_id: Some(" acct-a ".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9002/v2".to_string(),
                auth_token: None,
                account_id: None,
            },
        ],
    });

    assert_eq!(
        pool.snapshot(),
        GatewayWorkerPoolSnapshot {
            account_count: 1,
            leased_account_count: 1,
            policy_eligible_account_count: 1,
            policy_ineligible_account_count: 0,
            worker_slot_count: 2,
            bound_worker_slot_count: 1,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: true,
                policy_ineligibility_reason: None,
                cooldown_reason: None,
                last_error: None,
            }],
            worker_slots: vec![
                GatewayWorkerPoolSlot {
                    worker_id: 0,
                    websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                    account_id: Some("acct-a".to_string()),
                    account_login_state_path: None,
                },
                GatewayWorkerPoolSlot {
                    worker_id: 1,
                    websocket_url: "ws://127.0.0.1:9002/v2".to_string(),
                    account_id: None,
                    account_login_state_path: None,
                },
            ],
        }
    );
}

#[test]
fn snapshot_with_worker_health_reports_current_account_capacity() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![GatewayRemoteWorkerConfig {
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            auth_token: None,
            account_id: Some("acct-a".to_string()),
        }],
    };
    let pool = GatewayWorkerPoolState::from_remote_runtime(&remote_runtime);
    let worker_health = RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:9001/v2".to_string(),
        Some("acct-a".to_string()),
    )]);

    worker_health.mark_account_exhausted_for_worker(0, "quota exhausted".to_string());

    assert_eq!(
        pool.snapshot_with_worker_health(&worker_health),
        GatewayWorkerPoolSnapshot {
            account_count: 1,
            leased_account_count: 1,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Exhausted,
                account_capacity_reason: Some("quota exhausted".to_string()),
                policy_eligible: false,
                policy_ineligibility_reason: Some("quota exhausted".to_string()),
                cooldown_reason: None,
                last_error: None,
            }],
            worker_slots: vec![GatewayWorkerPoolSlot {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                account_id: Some("acct-a".to_string()),
                account_login_state_path: None,
            }],
        }
    );
}

#[test]
fn snapshot_with_worker_health_marks_unhealthy_leased_worker_ineligible() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![GatewayRemoteWorkerConfig {
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            auth_token: None,
            account_id: Some("acct-a".to_string()),
        }],
    };
    let pool = GatewayWorkerPoolState::from_remote_runtime(&remote_runtime);
    let worker_health = RemoteWorkerHealthRegistry::new_with_accounts(vec![(
        "ws://127.0.0.1:9001/v2".to_string(),
        Some("acct-a".to_string()),
    )]);

    worker_health.mark_unhealthy(0, Some("worker disconnected".to_string()));

    assert_eq!(
        pool.snapshot_with_worker_health(&worker_health),
        GatewayWorkerPoolSnapshot {
            account_count: 1,
            leased_account_count: 1,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: false,
                policy_ineligibility_reason: Some("leased worker is unhealthy".to_string()),
                cooldown_reason: None,
                last_error: None,
            }],
            worker_slots: vec![GatewayWorkerPoolSlot {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                account_id: Some("acct-a".to_string()),
                account_login_state_path: None,
            }],
        }
    );
}

#[test]
fn snapshot_with_worker_health_marks_missing_leased_worker_ineligible() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![GatewayRemoteWorkerConfig {
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            auth_token: None,
            account_id: Some("acct-a".to_string()),
        }],
    };
    let pool = GatewayWorkerPoolState::from_remote_runtime(&remote_runtime);
    let worker_health = RemoteWorkerHealthRegistry::new_with_accounts(Vec::new());

    assert_eq!(
        pool.snapshot_with_worker_health(&worker_health),
        GatewayWorkerPoolSnapshot {
            account_count: 1,
            leased_account_count: 1,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Exhausted,
                account_capacity_reason: Some("leased worker is missing".to_string()),
                policy_eligible: false,
                policy_ineligibility_reason: Some("leased worker is missing".to_string()),
                cooldown_reason: None,
                last_error: None,
            }],
            worker_slots: vec![GatewayWorkerPoolSlot {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                account_id: Some("acct-a".to_string()),
                account_login_state_path: None,
            }],
        }
    );
}

#[test]
fn snapshot_with_project_routes_reports_leased_account_route_counts() {
    let remote_runtime = GatewayRemoteRuntimeConfig {
        selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
        workers: vec![
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                auth_token: None,
                account_id: Some("acct-a".to_string()),
            },
            GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9002/v2".to_string(),
                auth_token: None,
                account_id: Some("acct-b".to_string()),
            },
        ],
    };
    let pool = GatewayWorkerPoolState::from_remote_runtime(&remote_runtime);
    let worker_health = RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://127.0.0.1:9001/v2".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://127.0.0.1:9002/v2".to_string(),
            Some("acct-b".to_string()),
        ),
    ]);

    let snapshot = pool.snapshot_with_worker_health_and_project_routes(
        &worker_health,
        &[
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-c".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
        ],
    );

    assert_eq!(
        snapshot
            .accounts
            .iter()
            .map(|account| (account.account_id.as_str(), account.project_route_count))
            .collect::<Vec<_>>(),
        vec![("acct-a", 2), ("acct-b", 1)]
    );
}
