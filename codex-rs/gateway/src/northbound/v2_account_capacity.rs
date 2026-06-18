use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::RateLimitSnapshot;
use codex_app_server_protocol::ServerNotification;

pub(crate) fn sync_worker_account_capacity_from_notification(
    downstream: &GatewayV2DownstreamRouter,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    notification: &ServerNotification,
) {
    let ServerNotification::AccountRateLimitsUpdated(notification) = notification else {
        return;
    };

    sync_worker_account_capacity_from_rate_limits(
        downstream,
        context,
        observability,
        worker_id,
        [&notification.rate_limits],
    );
}

pub(crate) fn sync_worker_account_capacity_from_rate_limits_response(
    downstream: &GatewayV2DownstreamRouter,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    response: &GetAccountRateLimitsResponse,
) {
    let mut snapshots = vec![&response.rate_limits];
    if let Some(rate_limits_by_limit_id) = &response.rate_limits_by_limit_id {
        snapshots.extend(rate_limits_by_limit_id.values());
    }
    sync_worker_account_capacity_from_rate_limits(
        downstream,
        context,
        observability,
        worker_id,
        snapshots,
    );
}

fn sync_worker_account_capacity_from_rate_limits<'a>(
    downstream: &GatewayV2DownstreamRouter,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    worker_id: Option<usize>,
    snapshots: impl IntoIterator<Item = &'a RateLimitSnapshot>,
) {
    let Some(worker_id) = worker_id else {
        return;
    };

    if let Some(reason) = account_capacity_exhaustion_reason(snapshots) {
        if downstream.mark_worker_account_exhausted(worker_id, reason.clone()) {
            observability.record_v2_account_capacity_event(
                worker_id,
                "exhausted",
                Some(context),
                Some(reason.as_str()),
            );
        }
    } else if downstream.mark_worker_account_available(worker_id) {
        observability.record_v2_account_capacity_event(
            worker_id,
            "available",
            Some(context),
            Some("account/rateLimits reported available capacity"),
        );
    }
}

fn account_capacity_exhaustion_reason<'a>(
    snapshots: impl IntoIterator<Item = &'a RateLimitSnapshot>,
) -> Option<String> {
    snapshots.into_iter().find_map(|snapshot| {
        snapshot.rate_limit_reached_type.map(|reached_type| {
            let limit_name = snapshot
                .limit_name
                .as_deref()
                .or(snapshot.limit_id.as_deref())
                .unwrap_or("account");
            format!("account/rateLimits reported {limit_name} {reached_type:?}")
        })
    })
}

#[cfg(test)]
mod tests {
    use codex_app_server_protocol::RateLimitReachedType;
    use codex_app_server_protocol::RateLimitSnapshot;
    use pretty_assertions::assert_eq;

    #[test]
    fn account_capacity_exhaustion_reason_uses_first_reached_limit() {
        let snapshots = [
            RateLimitSnapshot {
                limit_id: Some("codex".to_string()),
                limit_name: None,
                primary: None,
                secondary: None,
                credits: None,
                plan_type: None,
                rate_limit_reached_type: None,
                individual_limit: None,
            },
            RateLimitSnapshot {
                limit_id: Some("credits".to_string()),
                limit_name: Some("Credits".to_string()),
                primary: None,
                secondary: None,
                credits: None,
                plan_type: None,
                rate_limit_reached_type: Some(RateLimitReachedType::WorkspaceOwnerCreditsDepleted),
                individual_limit: None,
            },
        ];

        assert_eq!(
            super::account_capacity_exhaustion_reason(snapshots.iter()).as_deref(),
            Some("account/rateLimits reported Credits WorkspaceOwnerCreditsDepleted")
        );
    }
}
