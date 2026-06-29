use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn health_route_returns_runtime_health_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8(body.to_vec()).expect("utf8"),
        r#"{"status":"ok","runtimeMode":"embedded","executionMode":"inProcess","v2Compatibility":"embedded","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":10,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64,"maxPendingClientRequests":64},"v2Connections":{"activeConnectionCount":0,"activeConnectionPendingClientRequestCount":0,"activeConnectionMaxPendingClientRequestCount":0,"activeConnectionPeakPendingClientRequestCount":0,"activeConnectionPendingClientRequestStartedAt":null,"activeConnectionPendingClientRequestWorkerCounts":[],"activeConnectionPendingClientRequestMethodCounts":[],"activeConnectionPendingServerRequestCount":0,"activeConnectionAnsweredButUnresolvedServerRequestCount":0,"activeConnectionServerRequestBacklogCount":0,"activeConnectionMaxServerRequestBacklogCount":0,"activeConnectionPeakServerRequestBacklogCount":0,"activeConnectionServerRequestBacklogStartedAt":null,"activeConnectionServerRequestBacklogWorkerCounts":[],"activeConnectionServerRequestBacklogMethodCounts":[],"projectWorkerRouteSelectionCount":0,"projectWorkerRouteSelectionWorkerCounts":[],"lastProjectWorkerRouteSelectedWorkerId":null,"lastProjectWorkerRouteSelectedTenantId":null,"lastProjectWorkerRouteSelectedProjectId":null,"lastProjectWorkerRouteSelectedThreadId":null,"lastProjectWorkerRouteSelectedAccountId":null,"lastProjectWorkerRouteSelectedAt":null,"accountCapacityEventCounts":{},"accountCapacityEventWorkerCounts":[],"lastAccountCapacityEvent":null,"lastAccountCapacityEventWorkerId":null,"lastAccountCapacityEventTenantId":null,"lastAccountCapacityEventProjectId":null,"lastAccountCapacityEventReason":null,"lastAccountCapacityEventAt":null,"workerReconnectEventCounts":{},"workerReconnectEventWorkerCounts":[],"lastWorkerReconnectEvent":null,"lastWorkerReconnectEventWorkerId":null,"lastWorkerReconnectEventAt":null,"requestCounts":[],"lastRequestMethod":null,"lastRequestOutcome":null,"lastRequestDurationMs":null,"maxRequestDurationMs":null,"lastRequestAt":null,"clientRequestRejectionCounts":[],"lastClientRequestRejectionMethod":null,"lastClientRequestRejectionReason":null,"lastClientRequestRejectionAt":null,"serverRequestRejectionCounts":[],"lastServerRequestRejectionMethod":null,"lastServerRequestRejectionReason":null,"lastServerRequestRejectionAt":null,"serverRequestLifecycleEventCounts":[],"lastServerRequestLifecycleEvent":null,"lastServerRequestLifecycleMethod":null,"lastServerRequestLifecycleAt":null,"failClosedRequestCounts":[],"lastFailClosedRequestMethod":null,"lastFailClosedRequestReconnectBackoffActive":null,"lastFailClosedRequestAt":null,"upstreamRequestFailureCounts":[],"lastUpstreamRequestFailureMethod":null,"lastUpstreamRequestFailureReconnectBackoffActive":null,"lastUpstreamRequestFailureAt":null,"downstreamBackpressureCounts":[],"lastDownstreamBackpressureWorkerId":null,"lastDownstreamBackpressureAt":null,"clientSendTimeoutCount":0,"lastClientSendTimeoutAt":null,"threadListDeduplicationCounts":[],"lastThreadListDeduplicationSelectedWorkerId":null,"lastThreadListDeduplicationAt":null,"threadRouteRecoveryCounts":[],"lastThreadRouteRecoveryOutcome":null,"lastThreadRouteRecoveryAt":null,"degradedThreadDiscoveryCounts":[],"lastDegradedThreadDiscoveryMethod":null,"lastDegradedThreadDiscoveryReconnectBackoffActive":null,"lastDegradedThreadDiscoveryAt":null,"forwardedNotificationCounts":[],"lastForwardedNotificationMethod":null,"lastForwardedNotificationAt":null,"notificationSendFailureCounts":[],"lastNotificationSendFailureMethod":null,"lastNotificationSendFailureOutcome":null,"lastNotificationSendFailureAt":null,"clientResponseSendFailureCounts":[],"lastClientResponseSendFailureMethod":null,"lastClientResponseSendFailureOutcome":null,"lastClientResponseSendFailureAt":null,"downstreamShutdownFailureCounts":[],"lastDownstreamShutdownFailureOutcome":null,"lastDownstreamShutdownFailureAt":null,"closeFrameSendFailureCounts":[],"lastCloseFrameSendFailureCode":null,"lastCloseFrameSendFailureOutcome":null,"lastCloseFrameSendFailureAt":null,"serverRequestForwardSendFailureCounts":[],"lastServerRequestForwardSendFailureMethod":null,"lastServerRequestForwardSendFailureOutcome":null,"lastServerRequestForwardSendFailureAt":null,"serverRequestAnswerDeliveryFailureCounts":[],"lastServerRequestAnswerDeliveryFailureResponseKind":null,"lastServerRequestAnswerDeliveryFailureAt":null,"serverRequestRejectionDeliveryFailureCounts":[],"lastServerRequestRejectionDeliveryFailureMethod":null,"lastServerRequestRejectionDeliveryFailureAt":null,"suppressedNotificationCounts":[],"lastSuppressedNotificationMethod":null,"lastSuppressedNotificationReason":null,"lastSuppressedNotificationAt":null,"protocolViolationCounts":[],"protocolViolationWorkerCounts":[],"lastProtocolViolationPhase":null,"lastProtocolViolationReason":null,"lastProtocolViolationWorkerId":null,"lastProtocolViolationAt":null,"connectionOutcomeCounts":[],"peakActiveConnectionCount":0,"totalConnectionCount":0,"lastConnectionStartedAt":null,"lastConnectionCompletedAt":null,"lastConnectionDurationMs":null,"maxConnectionDurationMs":null,"lastConnectionOutcome":null,"lastConnectionDetail":null,"lastConnectionPendingClientRequestCount":0,"lastConnectionMaxPendingClientRequestCount":0,"lastConnectionPendingClientRequestStartedAt":null,"lastConnectionPendingClientRequestWorkerCounts":[],"lastConnectionPendingClientRequestMethodCounts":[],"lastConnectionPendingServerRequestCount":0,"lastConnectionAnsweredButUnresolvedServerRequestCount":0,"lastConnectionServerRequestBacklogCount":0,"lastConnectionMaxServerRequestBacklogCount":0,"lastConnectionServerRequestBacklogStartedAt":null,"lastConnectionServerRequestBacklogWorkerCounts":[],"lastConnectionServerRequestBacklogMethodCounts":[]},"pendingServerRequestCount":0,"pendingServerRequestKindCounts":{},"pendingServerRequestRouteCounts":[],"pendingServerRequestOldestAt":null,"remoteWorkers":null,"remoteAccountLabelsComplete":null,"remoteUnlabeledAccountWorkerCount":null,"remoteUnlabeledAccountWorkerIds":null,"remoteUnlabeledAccountWorkers":null,"projectWorkerRoutes":null}"#
    );
}

#[tokio::test]
async fn health_route_serializes_incomplete_remote_account_labels() {
    let app = router(
        Arc::new(FakeRuntime {
            health_response: Some(GatewayHealthResponse {
                status: GatewayHealthStatus::Ok,
                runtime_mode: "remote".to_string(),
                execution_mode: GatewayExecutionMode::WorkerManaged,
                v2_compatibility: GatewayV2CompatibilityMode::RemoteMultiWorker,
                v2_transport: GatewayV2TransportConfig {
                    initialize_timeout_seconds: 30,
                    client_send_timeout_seconds: 10,
                    reconnect_retry_backoff_seconds: 1,
                    max_pending_server_requests: 64,
                    max_pending_client_requests: 64,
                },
                v2_connections: default_v2_connections(),
                pending_server_request_count: 0,
                pending_server_request_kind_counts: std::collections::BTreeMap::new(),
                pending_server_request_route_counts: Vec::new(),
                pending_server_request_oldest_at: None,
                remote_workers: Some(vec![
                    GatewayRemoteWorkerHealth {
                        worker_id: 0,
                        websocket_url: "ws://127.0.0.1:8081".to_string(),
                        account_id: Some("acct-a".to_string()),
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                        account_capacity_reason: None,
                        account_capacity_last_changed_at: None,
                        healthy: true,
                        reconnecting: false,
                        reconnect_attempt_count: 0,
                        last_error: None,
                        last_state_change_at: Some(1710000000),
                        last_error_at: None,
                        next_reconnect_at: None,
                        reconnect_backoff_remaining_seconds: None,
                    },
                    GatewayRemoteWorkerHealth {
                        worker_id: 1,
                        websocket_url: "ws://127.0.0.1:8082".to_string(),
                        account_id: None,
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                        account_capacity_reason: None,
                        account_capacity_last_changed_at: None,
                        healthy: true,
                        reconnecting: false,
                        reconnect_attempt_count: 0,
                        last_error: None,
                        last_state_change_at: Some(1710000000),
                        last_error_at: None,
                        next_reconnect_at: None,
                        reconnect_backoff_remaining_seconds: None,
                    },
                ]),
                remote_account_labels_complete: Some(false),
                remote_unlabeled_account_worker_count: Some(1),
                remote_unlabeled_account_worker_ids: Some(vec![1]),
                remote_unlabeled_account_workers: Some(vec![GatewayRemoteUnlabeledAccountWorker {
                    worker_id: 1,
                    websocket_url: "ws://127.0.0.1:8082".to_string(),
                }]),
                project_worker_routes: Some(Vec::new()),
            }),
            ..FakeRuntime::default()
        }),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("health response should be JSON");
    assert_eq!(body["remoteAccountLabelsComplete"], false);
    assert_eq!(body["remoteUnlabeledAccountWorkerCount"], 1);
    assert_eq!(
        body["remoteUnlabeledAccountWorkerIds"],
        serde_json::json!([1])
    );
    assert_eq!(
        body["remoteUnlabeledAccountWorkers"],
        serde_json::json!([{"workerId":1,"websocketUrl":"ws://127.0.0.1:8082"}])
    );
    assert_eq!(
        body["remoteWorkers"][1]["accountId"],
        serde_json::Value::Null
    );
}
