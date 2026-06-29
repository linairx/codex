use super::support::*;
use pretty_assertions::assert_eq;

#[test]
fn snapshot_tracks_suppressed_notifications() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_suppressed_notification("skills/changed", "pending_refresh");
    registry.record_suppressed_notification("skills/changed", "pending_refresh");
    registry.record_suppressed_notification("warning", "client_opt_out");

    let snapshot = registry.snapshot();

    assert_eq!(
        snapshot.suppressed_notification_counts,
        vec![
            GatewayV2SuppressedNotificationCounts {
                method: "skills/changed".to_string(),
                reason: "pending_refresh".to_string(),
                count: 2,
            },
            GatewayV2SuppressedNotificationCounts {
                method: "warning".to_string(),
                reason: "client_opt_out".to_string(),
                count: 1,
            },
        ]
    );
    assert_eq!(
        snapshot.last_suppressed_notification_method,
        Some("warning".to_string())
    );
    assert_eq!(
        snapshot.last_suppressed_notification_reason,
        Some("client_opt_out".to_string())
    );
    assert_eq!(snapshot.last_suppressed_notification_at.is_some(), true);
}

#[test]
fn snapshot_tracks_forwarded_notifications() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_forwarded_notification("warning");
    registry.record_forwarded_notification("item/agentMessage/delta");
    registry.record_forwarded_notification("item/agentMessage/delta");

    let snapshot = registry.snapshot();

    assert_eq!(
        snapshot.forwarded_notification_counts,
        vec![
            GatewayV2ForwardedNotificationCounts {
                method: "item/agentMessage/delta".to_string(),
                count: 2,
            },
            GatewayV2ForwardedNotificationCounts {
                method: "warning".to_string(),
                count: 1,
            },
        ]
    );
    assert_eq!(
        snapshot.last_forwarded_notification_method,
        Some("item/agentMessage/delta".to_string())
    );
    assert_eq!(snapshot.last_forwarded_notification_at.is_some(), true);
}

#[test]
fn snapshot_tracks_notification_send_failures() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_notification_send_failure("warning", "client_send_timed_out");
    registry.record_notification_send_failure("warning", "client_send_timed_out");
    registry.record_notification_send_failure("serverRequest/resolved", "send_failed");

    let snapshot = registry.snapshot();

    assert_eq!(
        snapshot.notification_send_failure_counts,
        vec![
            GatewayV2NotificationSendFailureCounts {
                method: "serverRequest/resolved".to_string(),
                outcome: "send_failed".to_string(),
                count: 1,
            },
            GatewayV2NotificationSendFailureCounts {
                method: "warning".to_string(),
                outcome: "client_send_timed_out".to_string(),
                count: 2,
            },
        ]
    );
    assert_eq!(
        snapshot.last_notification_send_failure_method,
        Some("serverRequest/resolved".to_string())
    );
    assert_eq!(
        snapshot.last_notification_send_failure_outcome,
        Some("send_failed".to_string())
    );
    assert_eq!(snapshot.last_notification_send_failure_at.is_some(), true);
}

#[test]
fn snapshot_tracks_transport_and_server_request_delivery_failures() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_client_response_send_failure("model/list", "client_send_timed_out");
    registry.record_client_response_send_failure("model/list", "client_send_timed_out");
    registry.record_downstream_shutdown_failure("client_send_timed_out");
    registry.record_close_frame_send_failure(1008, "client_disconnected");
    registry.record_server_request_forward_send_failure(
        "item/tool/requestUserInput",
        "client_send_timed_out",
    );
    registry.record_server_request_forward_send_failure(
        "item/tool/requestUserInput",
        "client_send_timed_out",
    );
    registry.record_server_request_answer_delivery_failure("response");
    registry.record_server_request_answer_delivery_failure("error");
    registry.record_server_request_rejection_delivery_failure("item/permissions/requestApproval");

    let snapshot = registry.snapshot();

    assert_eq!(
        snapshot.client_response_send_failure_counts,
        vec![GatewayV2ClientResponseSendFailureCounts {
            method: "model/list".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 2,
        }]
    );
    assert_eq!(
        snapshot.last_client_response_send_failure_method,
        Some("model/list".to_string())
    );
    assert_eq!(
        snapshot.last_client_response_send_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(
        snapshot.last_client_response_send_failure_at.is_some(),
        true
    );
    assert_eq!(
        snapshot.downstream_shutdown_failure_counts,
        vec![GatewayV2DownstreamShutdownFailureCounts {
            outcome: "client_send_timed_out".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        snapshot.last_downstream_shutdown_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(snapshot.last_downstream_shutdown_failure_at.is_some(), true);
    assert_eq!(
        snapshot.close_frame_send_failure_counts,
        vec![GatewayV2CloseFrameSendFailureCounts {
            code: 1008,
            outcome: "client_disconnected".to_string(),
            count: 1,
        }]
    );
    assert_eq!(snapshot.last_close_frame_send_failure_code, Some(1008));
    assert_eq!(
        snapshot.last_close_frame_send_failure_outcome,
        Some("client_disconnected".to_string())
    );
    assert_eq!(snapshot.last_close_frame_send_failure_at.is_some(), true);
    assert_eq!(
        snapshot.server_request_forward_send_failure_counts,
        vec![GatewayV2ServerRequestForwardSendFailureCounts {
            method: "item/tool/requestUserInput".to_string(),
            outcome: "client_send_timed_out".to_string(),
            count: 2,
        }]
    );
    assert_eq!(
        snapshot.last_server_request_forward_send_failure_method,
        Some("item/tool/requestUserInput".to_string())
    );
    assert_eq!(
        snapshot.last_server_request_forward_send_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_eq!(
        snapshot
            .last_server_request_forward_send_failure_at
            .is_some(),
        true
    );
    assert_eq!(
        snapshot.server_request_answer_delivery_failure_counts,
        vec![
            GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                response_kind: "error".to_string(),
                count: 1,
            },
            GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                response_kind: "response".to_string(),
                count: 1,
            },
        ]
    );
    assert_eq!(
        snapshot.last_server_request_answer_delivery_failure_response_kind,
        Some("error".to_string())
    );
    assert_eq!(
        snapshot
            .last_server_request_answer_delivery_failure_at
            .is_some(),
        true
    );
    assert_eq!(
        snapshot.server_request_rejection_delivery_failure_counts,
        vec![GatewayV2ServerRequestRejectionDeliveryFailureCounts {
            method: "item/permissions/requestApproval".to_string(),
            count: 1,
        }]
    );
    assert_eq!(
        snapshot.last_server_request_rejection_delivery_failure_method,
        Some("item/permissions/requestApproval".to_string())
    );
    assert_eq!(
        snapshot
            .last_server_request_rejection_delivery_failure_at
            .is_some(),
        true
    );
}
