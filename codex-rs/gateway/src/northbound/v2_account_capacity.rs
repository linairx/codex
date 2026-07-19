use crate::api::GatewayAccountLeaseState;
use crate::event::GatewayAccountActiveThreadHandoffFailed;
use crate::event::GatewayEvent;
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_request_routing_handoff::is_account_capacity_error;
use crate::northbound::v2_routing::worker_for_request;
use crate::northbound::v2_scope::apply_response_scope_policy;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RateLimitSnapshot;
use codex_app_server_protocol::ServerNotification;
use serde_json::Value;
use std::io;
use tracing::info;
use tracing::warn;

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
            publish_account_lease_changed(
                downstream,
                context,
                observability,
                worker_id,
                GatewayAccountLeaseState::Cooldown,
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
        publish_account_lease_changed(
            downstream,
            context,
            observability,
            worker_id,
            GatewayAccountLeaseState::Leased,
            None,
        );
    }
}

pub(crate) fn fail_closed_active_thread_request_if_account_exhausted(
    downstream: &GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: &JSONRPCRequest,
) -> io::Result<()> {
    if !downstream.multi_worker_topology()
        || crate::northbound::v2_routing::requires_primary_worker_route(request)
    {
        return Ok(());
    }

    let Some(thread_id) = crate::northbound::v2_scope::request_thread_id(request) else {
        return Ok(());
    };
    if !connection
        .scope_registry
        .thread_visible_to(connection.request_context, thread_id)
    {
        return Ok(());
    }
    let Ok(worker) = downstream.worker_for_thread(connection.scope_registry, thread_id) else {
        return Ok(());
    };
    let Some(worker_id) = worker.worker_id else {
        return Ok(());
    };
    if downstream.worker_account_has_capacity(worker_id) {
        return Ok(());
    }

    let message = format!(
        "thread {thread_id} is pinned to worker {worker_id} with exhausted account capacity for {}",
        request.method
    );
    connection.observability.record_v2_account_capacity_event(
        worker_id,
        "active_thread_handoff_failure",
        Some(connection.request_context),
        Some(message.as_str()),
    );
    connection.observability.publish_operator_event(
        GatewayEvent::account_active_thread_handoff_failed(
            GatewayAccountActiveThreadHandoffFailed {
                tenant_id: connection.request_context.tenant_id.as_str(),
                project_id: connection.request_context.project_id.as_deref(),
                method: request.method.as_str(),
                thread_id,
                exhausted_worker_id: worker_id,
                exhausted_account_id: downstream.worker_account_id(worker_id),
                reason: message.as_str(),
            },
        ),
    );
    tracing::warn!(
        method = request.method.as_str(),
        account_capacity_event = "active_thread_handoff_failure",
        tenant_id = connection.request_context.tenant_id.as_str(),
        project_id = connection.request_context.project_id.as_deref(),
        thread_id,
        exhausted_worker_id = worker_id,
        exhausted_account_id = downstream.worker_account_id(worker_id),
        "gateway v2 failed closed for an active thread request pinned to an exhausted account because no protocol-visible context handoff was requested"
    );
    Err(io::Error::other(FailClosedMultiWorkerRouteError {
        message,
    }))
}

pub(crate) async fn route_thread_start_request_with_account_capacity_failover(
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let max_attempts = downstream.workers.len().max(1);
    let mut first_capacity_error = None;
    let mut exhausted_worker_ids = Vec::new();

    for _ in 0..max_attempts {
        let worker = worker_for_request(
            downstream,
            connection.scope_registry,
            connection.request_context,
            &request,
        )?
        .clone();
        let worker_account_id = worker
            .worker_id
            .and_then(|worker_id| downstream.worker_account_id(worker_id));
        let method = request.method.clone();
        let response = worker
            .request_handle
            .request(jsonrpc_request_to_client_request(request.clone())?)
            .await?;

        match response {
            Ok(result) => {
                if let Some(worker_id) = worker.worker_id {
                    if downstream.mark_worker_account_available(worker_id) {
                        publish_account_lease_changed(
                            downstream,
                            connection.request_context,
                            connection.observability,
                            worker_id,
                            GatewayAccountLeaseState::Leased,
                            None,
                        );
                    }
                    if !exhausted_worker_ids.is_empty() {
                        connection.observability.publish_operator_event(
                            GatewayEvent::account_failover_succeeded(
                                connection.request_context.tenant_id.as_str(),
                                connection.request_context.project_id.as_deref(),
                                worker_id,
                                downstream.worker_account_id(worker_id),
                                exhausted_worker_ids.clone(),
                            ),
                        );
                        connection.observability.record_v2_account_capacity_event(
                            worker_id,
                            "thread_start_failover_success",
                            Some(connection.request_context),
                            Some("thread/start restored on a replacement account-backed worker"),
                        );
                        info!(
                            method = method.as_str(),
                            tenant_id = connection.request_context.tenant_id.as_str(),
                            project_id = connection.request_context.project_id.as_deref(),
                            replacement_worker_id = worker_id,
                            replacement_account_id = downstream.worker_account_id(worker_id),
                            exhausted_worker_ids = ?exhausted_worker_ids,
                            "gateway v2 thread/start retried on another account-backed worker after account capacity exhaustion"
                        );
                    }
                }
                return Ok(Ok(apply_response_scope_policy(
                    connection.scope_registry,
                    connection.request_context,
                    Some(connection.observability),
                    &method,
                    worker.worker_id,
                    worker_account_id,
                    result,
                )?));
            }
            Err(error) if is_account_capacity_error(&error) => {
                if let Some(worker_id) = worker.worker_id {
                    connection.observability.record_v2_account_capacity_event(
                        worker_id,
                        "exhausted",
                        Some(connection.request_context),
                        Some(error.message.as_str()),
                    );
                    warn!(
                        method = method.as_str(),
                        tenant_id = connection.request_context.tenant_id.as_str(),
                        project_id = connection.request_context.project_id.as_deref(),
                        exhausted_worker_id = worker_id,
                        exhausted_account_id = downstream.worker_account_id(worker_id),
                        reason = error.message.as_str(),
                        "gateway v2 marked account-backed worker exhausted after downstream thread/start failure"
                    );
                    connection.observability.publish_operator_event(
                        GatewayEvent::account_capacity_exhausted(
                            connection.request_context.tenant_id.as_str(),
                            connection.request_context.project_id.as_deref(),
                            worker_id,
                            downstream.worker_account_id(worker_id),
                            error.message.as_str(),
                        ),
                    );
                    if downstream.mark_worker_account_exhausted(worker_id, error.message.clone()) {
                        publish_account_lease_changed(
                            downstream,
                            connection.request_context,
                            connection.observability,
                            worker_id,
                            GatewayAccountLeaseState::Cooldown,
                            Some(error.message.as_str()),
                        );
                    }
                    exhausted_worker_ids.push(worker_id);
                }
                if first_capacity_error.is_none() {
                    first_capacity_error = Some(error);
                }
            }
            Err(error) => return Ok(Err(error)),
        }
    }

    first_capacity_error.map(Err).ok_or_else(|| {
        io::Error::other(
            "gateway v2 connection has no downstream app-server sessions with available account capacity",
        )
    })
}

fn publish_account_lease_changed(
    downstream: &GatewayV2DownstreamRouter,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    worker_id: usize,
    lease_state: GatewayAccountLeaseState,
    reason: Option<&str>,
) {
    let account_id = downstream.worker_account_id(worker_id);
    observability.record_account_lease_event(
        lease_state,
        account_id.as_deref(),
        Some(worker_id),
        Some(context),
        reason,
    );
    observability.publish_operator_event(GatewayEvent::account_lease_changed(
        context.tenant_id.as_str(),
        context.project_id.as_deref(),
        worker_id,
        account_id,
        lease_state,
        reason,
    ));
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
                spend_control_reached: None,
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
                spend_control_reached: None,
            },
        ];

        assert_eq!(
            super::account_capacity_exhaustion_reason(snapshots.iter()).as_deref(),
            Some("account/rateLimits reported Credits WorkspaceOwnerCreditsDepleted")
        );
    }
}
