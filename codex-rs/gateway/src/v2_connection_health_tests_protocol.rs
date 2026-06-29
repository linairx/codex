use super::support::*;
use pretty_assertions::assert_eq;

#[test]
fn snapshot_tracks_protocol_violations() {
    let registry = GatewayV2ConnectionHealthRegistry::default();

    registry.record_protocol_violation("pre_initialize", "initialize_order");
    registry.record_protocol_violation("post_initialize", "invalid_jsonrpc");
    registry.record_protocol_violation("pre_initialize", "initialize_order");
    registry.record_protocol_violation_for_worker("downstream", "invalid_jsonrpc", Some(2));
    registry.record_protocol_violation_for_worker("downstream", "invalid_jsonrpc", Some(2));
    registry.record_protocol_violation_for_worker("downstream", "invalid_frame", Some(3));

    let snapshot = registry.snapshot();

    assert_eq!(
        snapshot.protocol_violation_counts,
        vec![
            GatewayV2ProtocolViolationCounts {
                phase: "downstream".to_string(),
                reason: "invalid_frame".to_string(),
                count: 1,
            },
            GatewayV2ProtocolViolationCounts {
                phase: "downstream".to_string(),
                reason: "invalid_jsonrpc".to_string(),
                count: 2,
            },
            GatewayV2ProtocolViolationCounts {
                phase: "post_initialize".to_string(),
                reason: "invalid_jsonrpc".to_string(),
                count: 1,
            },
            GatewayV2ProtocolViolationCounts {
                phase: "pre_initialize".to_string(),
                reason: "initialize_order".to_string(),
                count: 2,
            },
        ]
    );
    assert_eq!(
        snapshot.protocol_violation_worker_counts,
        vec![
            crate::api::GatewayV2ProtocolViolationWorkerCounts {
                worker_id: 2,
                violation_counts: vec![GatewayV2ProtocolViolationCounts {
                    phase: "downstream".to_string(),
                    reason: "invalid_jsonrpc".to_string(),
                    count: 2,
                }],
            },
            crate::api::GatewayV2ProtocolViolationWorkerCounts {
                worker_id: 3,
                violation_counts: vec![GatewayV2ProtocolViolationCounts {
                    phase: "downstream".to_string(),
                    reason: "invalid_frame".to_string(),
                    count: 1,
                }],
            },
        ]
    );
    assert_eq!(
        snapshot.last_protocol_violation_phase,
        Some("downstream".to_string())
    );
    assert_eq!(
        snapshot.last_protocol_violation_reason,
        Some("invalid_frame".to_string())
    );
    assert_eq!(snapshot.last_protocol_violation_worker_id, Some(3));
    assert_eq!(snapshot.last_protocol_violation_at.is_some(), true);
}
