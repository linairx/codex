use crate::northbound::v2_connection::ForwardedConnectionNotification;
use codex_app_server_protocol::JSONRPCNotification;
use std::collections::HashMap;
use std::collections::VecDeque;

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
