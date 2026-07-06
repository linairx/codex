use super::*;
use crate::config::GatewayRemoteSelectionPolicy;
use crate::config::GatewayRemoteWorkerConfig;
use pretty_assertions::assert_eq;

fn static_slot(
    worker_id: usize,
    websocket_url: &str,
    account_id: Option<&str>,
    account_login_state_path: Option<&str>,
) -> GatewayWorkerPoolSlot {
    GatewayWorkerPoolSlot {
        worker_id,
        websocket_url: websocket_url.to_string(),
        account_id: account_id.map(str::to_string),
        account_login_state_path: account_login_state_path.map(str::to_string),
        account_capacity: None,
        account_capacity_reason: None,
        healthy: None,
        reconnecting: None,
        last_error: None,
    }
}

fn health_slot(
    worker_id: usize,
    websocket_url: &str,
    account_id: Option<&str>,
    healthy: bool,
    last_error: Option<&str>,
) -> GatewayWorkerPoolSlot {
    GatewayWorkerPoolSlot {
        worker_id,
        websocket_url: websocket_url.to_string(),
        account_id: account_id.map(str::to_string),
        account_login_state_path: None,
        account_capacity: account_id.map(|_| GatewayAccountCapacityStatus::Available),
        account_capacity_reason: None,
        healthy: Some(healthy),
        reconnecting: Some(false),
        last_error: last_error.map(str::to_string),
    }
}

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
            available_account_count: 0,
            leased_account_count: 1,
            cooldown_account_count: 0,
            policy_eligible_account_count: 1,
            policy_ineligible_account_count: 0,
            worker_slot_count: 2,
            bound_worker_slot_count: 1,
            healthy_worker_slot_count: 0,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
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
                static_slot(0, "ws://127.0.0.1:9001/v2", Some("acct-a"), None),
                static_slot(1, "ws://127.0.0.1:9002/v2", None, None),
            ],
        }
    );
}

#[test]
fn static_account_pool_reports_available_unleased_accounts() {
    let pool = GatewayWorkerPoolState::from_remote_runtime_with_account_pool(
        &GatewayRemoteRuntimeConfig {
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            workers: vec![GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                auth_token: None,
                account_id: Some("acct-a".to_string()),
            }],
        },
        &[
            " acct-a ".to_string(),
            " acct-b ".to_string(),
            "acct-b".to_string(),
            "   ".to_string(),
        ],
        &[],
        &[],
    );

    assert_eq!(
        pool.snapshot().accounts,
        vec![
            GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: true,
                policy_ineligibility_reason: None,
                cooldown_reason: None,
                last_error: None,
            },
            GatewayAccountPoolEntry {
                account_id: "acct-b".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Available,
                leased_worker_id: None,
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: false,
                policy_ineligibility_reason: Some("account is not leased to a worker".to_string()),
                cooldown_reason: None,
                last_error: None,
            },
        ]
    );
    assert_eq!(pool.snapshot().account_count, 2);
    assert_eq!(pool.snapshot().available_account_count, 1);
    assert_eq!(pool.snapshot().leased_account_count, 1);
    assert_eq!(pool.snapshot().policy_ineligible_account_count, 1);
}

#[test]
fn static_account_pool_reports_available_account_login_state_paths() {
    let pool = GatewayWorkerPoolState::from_remote_runtime_with_account_pool(
        &GatewayRemoteRuntimeConfig {
            selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
            workers: vec![GatewayRemoteWorkerConfig {
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                auth_token: None,
                account_id: Some("acct-a".to_string()),
            }],
        },
        &["acct-a".to_string(), "acct-b".to_string()],
        &[],
        &[
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ],
    );

    assert_eq!(
        pool.snapshot().accounts,
        vec![
            GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: Some("/codex-home/acct-a".to_string()),
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: true,
                policy_ineligibility_reason: None,
                cooldown_reason: None,
                last_error: None,
            },
            GatewayAccountPoolEntry {
                account_id: "acct-b".to_string(),
                account_login_state_path: Some("/codex-home/acct-b".to_string()),
                lease_state: GatewayAccountLeaseState::Available,
                leased_worker_id: None,
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: false,
                policy_ineligibility_reason: Some("account is not leased to a worker".to_string()),
                cooldown_reason: None,
                last_error: None,
            },
        ]
    );
    assert_eq!(pool.snapshot().account_login_state_path_count, 2);
    assert_eq!(pool.snapshot().worker_account_login_state_path_count, 0);
    assert_eq!(pool.snapshot().pool_account_login_state_path_count, 2);
}

#[test]
fn static_remote_runtime_reports_configured_account_login_state_paths() {
    let pool = GatewayWorkerPoolState::from_remote_runtime_with_account_login_state_paths(
        &GatewayRemoteRuntimeConfig {
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
        },
        &[
            Some("/codex-home/acct-a".to_string()),
            Some("/codex-home/acct-b".to_string()),
        ],
    );

    assert_eq!(
        pool.snapshot(),
        GatewayWorkerPoolSnapshot {
            account_count: 2,
            available_account_count: 0,
            leased_account_count: 2,
            cooldown_account_count: 0,
            policy_eligible_account_count: 2,
            policy_ineligible_account_count: 0,
            worker_slot_count: 2,
            bound_worker_slot_count: 2,
            healthy_worker_slot_count: 0,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 2,
            worker_account_login_state_path_count: 2,
            pool_account_login_state_path_count: 0,
            accounts: vec![
                GatewayAccountPoolEntry {
                    account_id: "acct-a".to_string(),
                    account_login_state_path: Some("/codex-home/acct-a".to_string()),
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(0),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
                GatewayAccountPoolEntry {
                    account_id: "acct-b".to_string(),
                    account_login_state_path: Some("/codex-home/acct-b".to_string()),
                    lease_state: GatewayAccountLeaseState::Leased,
                    leased_worker_id: Some(1),
                    project_route_count: 0,
                    account_capacity: GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    policy_eligible: true,
                    policy_ineligibility_reason: None,
                    cooldown_reason: None,
                    last_error: None,
                },
            ],
            worker_slots: vec![
                static_slot(
                    0,
                    "ws://127.0.0.1:9001/v2",
                    Some("acct-a"),
                    Some("/codex-home/acct-a"),
                ),
                static_slot(
                    1,
                    "ws://127.0.0.1:9002/v2",
                    Some("acct-b"),
                    Some("/codex-home/acct-b"),
                ),
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
            available_account_count: 0,
            leased_account_count: 0,
            cooldown_account_count: 1,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            healthy_worker_slot_count: 1,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Cooldown,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Exhausted,
                account_capacity_reason: Some("quota exhausted".to_string()),
                policy_eligible: false,
                policy_ineligibility_reason: Some("quota exhausted".to_string()),
                cooldown_reason: Some("quota exhausted".to_string()),
                last_error: None,
            }],
            worker_slots: vec![GatewayWorkerPoolSlot {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                account_id: Some("acct-a".to_string()),
                account_login_state_path: None,
                account_capacity: Some(GatewayAccountCapacityStatus::Exhausted),
                account_capacity_reason: Some("quota exhausted".to_string()),
                healthy: Some(true),
                reconnecting: Some(false),
                last_error: None,
            }],
        }
    );
}

#[test]
fn snapshot_with_worker_health_restores_cooldown_account_when_capacity_returns() {
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
    worker_health.mark_account_available_for_worker(0);

    assert_eq!(
        pool.snapshot_with_worker_health(&worker_health),
        GatewayWorkerPoolSnapshot {
            account_count: 1,
            available_account_count: 0,
            leased_account_count: 1,
            cooldown_account_count: 0,
            policy_eligible_account_count: 1,
            policy_ineligible_account_count: 0,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            healthy_worker_slot_count: 1,
            unhealthy_worker_slot_count: 0,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
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
            worker_slots: vec![health_slot(
                0,
                "ws://127.0.0.1:9001/v2",
                Some("acct-a"),
                true,
                None
            )],
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
            available_account_count: 0,
            leased_account_count: 1,
            cooldown_account_count: 0,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            healthy_worker_slot_count: 0,
            unhealthy_worker_slot_count: 1,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Available,
                account_capacity_reason: None,
                policy_eligible: false,
                policy_ineligibility_reason: Some("leased worker is unhealthy".to_string()),
                cooldown_reason: None,
                last_error: Some("worker disconnected".to_string()),
            }],
            worker_slots: vec![health_slot(
                0,
                "ws://127.0.0.1:9001/v2",
                Some("acct-a"),
                false,
                Some("worker disconnected")
            )],
        }
    );
}

#[test]
fn snapshot_with_worker_health_reports_reconnecting_worker_slots() {
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

    worker_health.mark_reconnecting(
        0,
        Some("remote app server event stream ended".to_string()),
        std::time::Duration::from_secs(1),
    );

    assert_eq!(
        pool.snapshot_with_worker_health(&worker_health)
            .worker_slots,
        vec![GatewayWorkerPoolSlot {
            worker_id: 0,
            websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
            account_id: Some("acct-a".to_string()),
            account_login_state_path: None,
            account_capacity: Some(GatewayAccountCapacityStatus::Available),
            account_capacity_reason: None,
            healthy: Some(false),
            reconnecting: Some(true),
            last_error: Some("remote app server event stream ended".to_string()),
        }]
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
            available_account_count: 0,
            leased_account_count: 1,
            cooldown_account_count: 0,
            policy_eligible_account_count: 0,
            policy_ineligible_account_count: 1,
            worker_slot_count: 1,
            bound_worker_slot_count: 1,
            healthy_worker_slot_count: 0,
            unhealthy_worker_slot_count: 1,
            reconnecting_worker_slot_count: 0,
            account_login_state_path_count: 0,
            worker_account_login_state_path_count: 0,
            pool_account_login_state_path_count: 0,
            accounts: vec![GatewayAccountPoolEntry {
                account_id: "acct-a".to_string(),
                account_login_state_path: None,
                lease_state: GatewayAccountLeaseState::Leased,
                leased_worker_id: Some(0),
                project_route_count: 0,
                account_capacity: GatewayAccountCapacityStatus::Exhausted,
                account_capacity_reason: Some("leased worker is missing".to_string()),
                policy_eligible: false,
                policy_ineligibility_reason: Some("leased worker is missing".to_string()),
                cooldown_reason: None,
                last_error: Some("leased worker is missing".to_string()),
            }],
            worker_slots: vec![GatewayWorkerPoolSlot {
                worker_id: 0,
                websocket_url: "ws://127.0.0.1:9001/v2".to_string(),
                account_id: Some("acct-a".to_string()),
                account_login_state_path: None,
                account_capacity: None,
                account_capacity_reason: None,
                healthy: Some(false),
                reconnecting: Some(false),
                last_error: Some("worker slot is missing from health registry".to_string()),
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
