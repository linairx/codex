use super::*;
use crate::northbound::v2_notifications::MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD;
use crate::northbound::v2_notifications::forwarded_connection_notification_duplicate;
use crate::northbound::v2_notifications::record_forwarded_connection_notification;
use crate::northbound::v2_notifications::should_deduplicate_connection_notification;
use pretty_assertions::assert_eq;

#[test]
pub(crate) fn login_completed_notifications_are_treated_as_deduplicated_connection_state() {
    let notification = JSONRPCNotification {
        method: "account/login/completed".to_string(),
        params: Some(serde_json::json!({
            "loginId": "login-shared",
            "success": true,
            "error": null,
        })),
    };

    assert_eq!(
        should_deduplicate_connection_notification(&notification),
        true
    );
}

#[test]
pub(crate) fn mcp_oauth_login_completed_notifications_are_treated_as_deduplicated_connection_state()
{
    let notification = JSONRPCNotification {
        method: "mcpServer/oauthLogin/completed".to_string(),
        params: Some(serde_json::json!({
            "name": "shared-mcp",
            "success": true,
            "error": null,
        })),
    };

    assert_eq!(
        should_deduplicate_connection_notification(&notification),
        true
    );
}

#[test]
pub(crate) fn external_agent_import_completed_notifications_are_treated_as_deduplicated_connection_state()
 {
    let notification = JSONRPCNotification {
        method: "externalAgentConfig/import/completed".to_string(),
        params: Some(serde_json::json!({
            "importId": "import-1",
            "itemTypeResults": [],
        })),
    };

    assert_eq!(
        should_deduplicate_connection_notification(&notification),
        true
    );
}

#[test]
pub(crate) fn account_discovery_notifications_are_treated_as_deduplicated_connection_state() {
    for method in [
        "account/updated",
        "account/rateLimits/updated",
        "app/list/updated",
    ] {
        let notification = JSONRPCNotification {
            method: method.to_string(),
            params: Some(match method {
                "account/updated" => serde_json::json!({
                    "authMode": null,
                    "planType": null,
                }),
                "account/rateLimits/updated" => serde_json::json!({
                    "rateLimits": {
                        "limitId": "shared",
                        "limitName": "Shared",
                        "primary": null,
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    },
                }),
                "app/list/updated" => serde_json::json!({
                    "data": [],
                }),
                _ => unreachable!("method list should stay exhaustive"),
            }),
        };

        assert_eq!(
            should_deduplicate_connection_notification(&notification),
            true
        );
    }
}

#[test]
pub(crate) fn visible_connection_warning_notifications_are_treated_as_deduplicated_connection_state()
 {
    for method in ["warning", "configWarning", "deprecationNotice"] {
        let notification = JSONRPCNotification {
            method: method.to_string(),
            params: Some(match method {
                "warning" => serde_json::json!({
                    "message": "shared worker warning",
                    "threadId": null,
                }),
                "configWarning" => serde_json::json!({
                    "summary": "shared config warning",
                    "details": null,
                }),
                "deprecationNotice" => serde_json::json!({
                    "summary": "shared deprecation notice",
                    "details": null,
                }),
                _ => unreachable!("method list should stay exhaustive"),
            }),
        };

        assert_eq!(
            should_deduplicate_connection_notification(&notification),
            true
        );
    }
}

#[test]
pub(crate) fn windows_setup_notifications_are_treated_as_deduplicated_connection_state() {
    for method in [
        "windows/worldWritableWarning",
        "windowsSandbox/setupCompleted",
    ] {
        let notification = JSONRPCNotification {
            method: method.to_string(),
            params: Some(match method {
                "windows/worldWritableWarning" => serde_json::json!({
                    "samplePaths": ["/tmp/world-writable"],
                    "extraCount": 2,
                    "failedScan": false,
                }),
                "windowsSandbox/setupCompleted" => serde_json::json!({
                    "mode": "unelevated",
                    "success": false,
                    "error": "setup failed",
                }),
                _ => unreachable!("method list should stay exhaustive"),
            }),
        };

        assert_eq!(
            should_deduplicate_connection_notification(&notification),
            true
        );
    }
}

#[test]
pub(crate) fn forwarded_connection_notification_dedupe_remembers_interleaved_payloads() {
    let mut forwarded_connection_notifications = HashMap::new();
    let first_notice = JSONRPCNotification {
        method: "warning".to_string(),
        params: Some(serde_json::json!({
            "message": "first warning",
            "threadId": null,
        })),
    };
    let second_notice = JSONRPCNotification {
        method: "warning".to_string(),
        params: Some(serde_json::json!({
            "message": "second warning",
            "threadId": null,
        })),
    };

    assert!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(1),
            &first_notice,
        )
        .is_none()
    );
    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &first_notice,
    );
    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &second_notice,
    );

    assert_eq!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(2),
            &first_notice,
        )
        .map(|forwarded| forwarded.worker_id),
        Some(Some(1))
    );
    assert_eq!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(2),
            &second_notice,
        )
        .map(|forwarded| forwarded.worker_id),
        Some(Some(1))
    );
    assert!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(1),
            &first_notice,
        )
        .is_none()
    );
}

#[test]
pub(crate) fn forwarded_connection_notification_dedupe_bounds_payload_history() {
    let mut forwarded_connection_notifications = HashMap::new();
    let oldest_notice = JSONRPCNotification {
        method: "warning".to_string(),
        params: Some(serde_json::json!({
            "message": "warning-0",
            "threadId": null,
        })),
    };

    for index in 0..=MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD {
        record_forwarded_connection_notification(
            &mut forwarded_connection_notifications,
            Some(index),
            &JSONRPCNotification {
                method: "warning".to_string(),
                params: Some(serde_json::json!({
                    "message": format!("warning-{index}"),
                    "threadId": null,
                })),
            },
        );
    }

    let newest_index = MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD;
    let newest_notice = JSONRPCNotification {
        method: "warning".to_string(),
        params: Some(serde_json::json!({
            "message": format!("warning-{newest_index}"),
            "threadId": null,
        })),
    };
    assert!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD + 1),
            &oldest_notice,
        )
        .is_none()
    );
    assert_eq!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD + 1),
            &newest_notice,
        )
        .map(|forwarded| forwarded.worker_id),
        Some(Some(newest_index))
    );
    assert_eq!(
        forwarded_connection_notifications
            .get("warning")
            .expect("warning history should exist")
            .len(),
        MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD
    );
}

#[test]
pub(crate) fn forwarded_connection_notification_dedupe_refreshes_same_worker_repeats() {
    let mut forwarded_connection_notifications = HashMap::new();
    let repeated_notice = JSONRPCNotification {
        method: "warning".to_string(),
        params: Some(serde_json::json!({
            "message": "repeated warning",
            "threadId": null,
        })),
    };

    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &repeated_notice,
    );
    for index in 0..MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD {
        record_forwarded_connection_notification(
            &mut forwarded_connection_notifications,
            Some(index + 10),
            &JSONRPCNotification {
                method: "warning".to_string(),
                params: Some(serde_json::json!({
                    "message": format!("warning-{index}"),
                    "threadId": null,
                })),
            },
        );
    }

    assert!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(2),
            &repeated_notice,
        )
        .is_none()
    );
    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &repeated_notice,
    );
    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(999),
        &JSONRPCNotification {
            method: "warning".to_string(),
            params: Some(serde_json::json!({
                "message": "newer warning",
                "threadId": null,
            })),
        },
    );

    assert_eq!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(2),
            &repeated_notice,
        )
        .map(|forwarded| forwarded.worker_id),
        Some(Some(1))
    );
    assert_eq!(
        forwarded_connection_notifications
            .get("warning")
            .expect("warning history should exist")
            .len(),
        MAX_FORWARDED_CONNECTION_NOTIFICATION_PAYLOADS_PER_METHOD
    );
}

#[test]
pub(crate) fn forwarded_connection_notification_dedupe_preserves_same_worker_empty_payload_repeats()
{
    let mut forwarded_connection_notifications = HashMap::new();
    let repeated_notice = JSONRPCNotification {
        method: "externalAgentConfig/import/completed".to_string(),
        params: Some(serde_json::json!({
            "importId": "import-1",
            "itemTypeResults": [],
        })),
    };

    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &repeated_notice,
    );
    assert!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(1),
            &repeated_notice,
        )
        .is_none()
    );

    record_forwarded_connection_notification(
        &mut forwarded_connection_notifications,
        Some(1),
        &repeated_notice,
    );
    assert_eq!(
        forwarded_connection_notifications
            .get("externalAgentConfig/import/completed")
            .expect("import-completed history should exist")
            .len(),
        1
    );
    assert_eq!(
        forwarded_connection_notification_duplicate(
            &forwarded_connection_notifications,
            Some(2),
            &repeated_notice,
        )
        .map(|forwarded| forwarded.worker_id),
        Some(Some(1))
    );
}
