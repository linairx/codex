//! Worker-pool and account-lease metrics for `GatewayObservability`.

use super::*;

impl GatewayObservability {
    pub(crate) fn record_worker_pool_inventory(&self, snapshot: &GatewayWorkerPoolSnapshot) {
        let inventory = [
            ("accounts", snapshot.account_count),
            ("available_accounts", snapshot.available_account_count),
            ("leased_accounts", snapshot.leased_account_count),
            ("cooldown_accounts", snapshot.cooldown_account_count),
            (
                "policy_eligible_accounts",
                snapshot.policy_eligible_account_count,
            ),
            (
                "policy_ineligible_accounts",
                snapshot.policy_ineligible_account_count,
            ),
            ("worker_slots", snapshot.worker_slot_count),
            ("bound_worker_slots", snapshot.bound_worker_slot_count),
            ("healthy_worker_slots", snapshot.healthy_worker_slot_count),
            (
                "unhealthy_worker_slots",
                snapshot.unhealthy_worker_slot_count,
            ),
            (
                "reconnecting_worker_slots",
                snapshot.reconnecting_worker_slot_count,
            ),
            (
                "account_login_state_paths",
                snapshot.account_login_state_path_count,
            ),
            (
                "worker_account_login_state_paths",
                snapshot.worker_account_login_state_path_count,
            ),
            (
                "pool_account_login_state_paths",
                snapshot.pool_account_login_state_path_count,
            ),
        ];

        for (kind, value) in inventory {
            let tags = [("kind", kind)];
            if let Some(metrics) = &self.metrics
                && let Err(err) = metrics.gauge(
                    WORKER_POOL_INVENTORY_METRIC,
                    value.min(i64::MAX as usize) as i64,
                    &tags,
                )
            {
                tracing::warn!("failed to record gateway worker-pool inventory metric: {err}");
            }
        }
    }

    pub(crate) fn record_worker_pool_account_leases(&self, snapshot: &GatewayWorkerPoolSnapshot) {
        for account in &snapshot.accounts {
            self.record_account_lease_event(
                account.lease_state,
                Some(account.account_id.as_str()),
                account.leased_worker_id,
                None,
                account.cooldown_reason.as_deref(),
            );
        }
    }

    pub(crate) fn record_account_lease_event(
        &self,
        lease_state: GatewayAccountLeaseState,
        account_id: Option<&str>,
        worker_id: Option<usize>,
        context: Option<&GatewayRequestContext>,
        reason: Option<&str>,
    ) {
        let event = match lease_state {
            GatewayAccountLeaseState::Available => "available",
            GatewayAccountLeaseState::Leased => "leased",
            GatewayAccountLeaseState::Cooldown => "cooldown",
        };
        let worker_id =
            worker_id.map_or_else(|| "none".to_string(), |worker_id| worker_id.to_string());
        let account_id = account_id.unwrap_or("none");
        let tags = [
            ("event", event),
            ("account_id", account_id),
            ("worker_id", worker_id.as_str()),
        ];

        if let Some(metrics) = &self.metrics
            && let Err(err) = metrics.counter(ACCOUNT_LEASE_EVENT_COUNT_METRIC, 1, &tags)
        {
            tracing::warn!("failed to record gateway account lease event metric: {err}");
        }

        if self.audit_logs_enabled {
            tracing::event!(
                target: "codex_gateway.audit",
                Level::INFO,
                event,
                account_id,
                worker_id = worker_id.as_str(),
                tenant_id = context.map(|context| context.tenant_id.as_str()),
                project_id = context.and_then(|context| context.project_id.as_deref()),
                reason,
                "gateway account lease state observed",
            );
        }
    }
}
