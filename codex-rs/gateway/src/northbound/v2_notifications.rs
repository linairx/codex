use crate::northbound::v2_connection::ForwardedConnectionNotification;
use crate::northbound::v2_scope::notification_thread_id;
use crate::scope::GatewayRequestContext;
use codex_app_server_protocol::JSONRPCNotification;
use std::collections::HashMap;
use std::collections::VecDeque;
use tracing::warn;

pub(crate) const MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD: usize = 256;

pub(crate) fn should_deduplicate_connection_notification(
    notification: &JSONRPCNotification,
) -> bool {
    matches!(
        notification.method.as_str(),
        "account/updated"
            | "account/rateLimits/updated"
            | "account/login/completed"
            | "app/list/updated"
            | "mcpServer/oauthLogin/completed"
            | "warning"
            | "configWarning"
            | "deprecationNotice"
            | "mcpServer/startupStatus/updated"
            | "externalAgentConfig/import/completed"
            | "windows/worldWritableWarning"
            | "windowsSandbox/setupCompleted"
    )
}

pub(crate) fn forwarded_connection_notification_duplicate<'a>(
    forwarded_connection_notifications: &'a HashMap<
        String,
        VecDeque<ForwardedConnectionNotification>,
    >,
    worker_id: Option<usize>,
    notification: &JSONRPCNotification,
) -> Option<&'a ForwardedConnectionNotification> {
    forwarded_connection_notifications
        .get(&notification.method)
        .and_then(|notifications_by_method| {
            notifications_by_method.iter().find(|forwarded| {
                forwarded.worker_id != worker_id && forwarded.params == notification.params
            })
        })
}

pub(crate) fn record_forwarded_connection_notification(
    forwarded_connection_notifications: &mut HashMap<
        String,
        VecDeque<ForwardedConnectionNotification>,
    >,
    worker_id: Option<usize>,
    notification: &JSONRPCNotification,
) {
    let notifications_by_method = forwarded_connection_notifications
        .entry(notification.method.clone())
        .or_default();
    if let Some(existing_index) = notifications_by_method.iter().position(|forwarded| {
        forwarded.worker_id == worker_id && forwarded.params == notification.params
    }) {
        notifications_by_method.remove(existing_index);
    }
    if notifications_by_method.len() >= MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD {
        notifications_by_method.pop_front();
    }
    notifications_by_method.push_back(ForwardedConnectionNotification {
        worker_id,
        params: notification.params.clone(),
    });
}

pub(crate) fn log_suppressed_skills_changed_notification(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    notification: &JSONRPCNotification,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        method = notification.method,
        params = ?notification.params,
        "suppressing duplicate multi-worker skills/changed notification until the client refreshes skills/list"
    );
}

pub(crate) fn log_suppressed_opted_out_notification(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    notification: &JSONRPCNotification,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        method = notification.method,
        params = ?notification.params,
        "suppressing downstream notification opted out by northbound v2 client"
    );
}

pub(crate) fn log_suppressed_duplicate_connection_notification(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    original_worker_id: Option<usize>,
    original_worker_websocket_url: &str,
    notification: &JSONRPCNotification,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        original_worker_id = ?original_worker_id,
        original_worker_websocket_url,
        method = notification.method,
        params = ?notification.params,
        "suppressing exact-duplicate multi-worker connection notification"
    );
}

pub(crate) fn log_suppressed_hidden_thread_notification(
    request_context: &GatewayRequestContext,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
    notification: &JSONRPCNotification,
) {
    warn!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        worker_id = ?worker_id,
        worker_websocket_url,
        method = notification.method,
        thread_id = notification
            .params
            .as_ref()
            .and_then(notification_thread_id),
        params = ?notification.params,
        "suppressing downstream notification for a thread outside the gateway request scope"
    );
}
