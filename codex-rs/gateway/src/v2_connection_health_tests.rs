mod support {
    pub(super) use super::super::GatewayV2ConnectionHealthRegistry;
    pub(super) use super::super::GatewayV2ConnectionPendingCounts;
    pub(super) use crate::api::GatewayV2ClientRequestRejectionCounts;
    pub(super) use crate::api::GatewayV2ClientResponseSendFailureCounts;
    pub(super) use crate::api::GatewayV2CloseFrameSendFailureCounts;
    pub(super) use crate::api::GatewayV2ConnectionHealth;
    pub(super) use crate::api::GatewayV2ConnectionOutcomeCounts;
    pub(super) use crate::api::GatewayV2DegradedThreadDiscoveryCounts;
    pub(super) use crate::api::GatewayV2DownstreamBackpressureCounts;
    pub(super) use crate::api::GatewayV2DownstreamShutdownFailureCounts;
    pub(super) use crate::api::GatewayV2FailClosedRequestCounts;
    pub(super) use crate::api::GatewayV2ForwardedNotificationCounts;
    pub(super) use crate::api::GatewayV2NotificationSendFailureCounts;
    pub(super) use crate::api::GatewayV2PendingClientRequestMethodCounts;
    pub(super) use crate::api::GatewayV2PendingClientRequestWorkerCounts;
    pub(super) use crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts;
    pub(super) use crate::api::GatewayV2ProtocolViolationCounts;
    pub(super) use crate::api::GatewayV2RequestCounts;
    pub(super) use crate::api::GatewayV2ServerRequestAnswerDeliveryFailureCounts;
    pub(super) use crate::api::GatewayV2ServerRequestBacklogMethodCounts;
    pub(super) use crate::api::GatewayV2ServerRequestBacklogWorkerCounts;
    pub(super) use crate::api::GatewayV2ServerRequestForwardSendFailureCounts;
    pub(super) use crate::api::GatewayV2ServerRequestLifecycleEventCounts;
    pub(super) use crate::api::GatewayV2ServerRequestRejectionCounts;
    pub(super) use crate::api::GatewayV2ServerRequestRejectionDeliveryFailureCounts;
    pub(super) use crate::api::GatewayV2SuppressedNotificationCounts;
    pub(super) use crate::api::GatewayV2ThreadListDeduplicationCounts;
    pub(super) use crate::api::GatewayV2ThreadRouteRecoveryCounts;
    pub(super) use crate::api::GatewayV2UpstreamRequestFailureCounts;
    pub(super) use crate::api::GatewayV2WorkerReconnectWorkerEventCounts;
    pub(super) use std::collections::BTreeMap;
    pub(super) use std::time::Duration;
}

#[path = "v2_connection_health_tests_connection_lifecycle.rs"]
mod connection_lifecycle;

#[path = "v2_connection_health_tests_pending_counts.rs"]
mod pending_counts;

#[path = "v2_connection_health_tests_pending_peaks.rs"]
mod pending_peaks;

#[path = "v2_connection_health_tests_connection_outcomes.rs"]
mod connection_outcomes;

#[path = "v2_connection_health_tests_route_events.rs"]
mod route_events;

#[path = "v2_connection_health_tests_request_metrics.rs"]
mod request_metrics;

#[path = "v2_connection_health_tests_notification_transport.rs"]
mod notification_transport;

#[path = "v2_connection_health_tests_protocol.rs"]
mod protocol;
