use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn health_route_serializes_remote_worker_retry_metadata_in_camel_case() {
    struct RemoteHealthRuntime;

    #[async_trait]
    impl GatewayRuntime for RemoteHealthRuntime {
        async fn create_thread(
            &self,
            _context: GatewayRequestContext,
            _request: CreateThreadRequest,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("health-only runtime should not receive create_thread")
        }

        async fn list_threads(
            &self,
            _context: GatewayRequestContext,
            _request: ListThreadsRequest,
        ) -> Result<ListThreadsResponse, GatewayError> {
            unreachable!("health-only runtime should not receive list_threads")
        }

        async fn read_thread(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("health-only runtime should not receive read_thread")
        }

        async fn start_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _request: StartTurnRequest,
        ) -> Result<TurnResponse, GatewayError> {
            unreachable!("health-only runtime should not receive start_turn")
        }

        async fn interrupt_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _turn_id: String,
        ) -> Result<InterruptTurnResponse, GatewayError> {
            unreachable!("health-only runtime should not receive interrupt_turn")
        }

        async fn resolve_server_request(
            &self,
            _context: GatewayRequestContext,
            _request: ResolveServerRequestRequest,
        ) -> Result<ResolveServerRequestResponse, GatewayError> {
            unreachable!("health-only runtime should not receive resolve_server_request")
        }

        fn health(&self) -> GatewayHealthResponse {
            GatewayHealthResponse {
                status: GatewayHealthStatus::Degraded,
                runtime_mode: "remote".to_string(),
                execution_mode: GatewayExecutionMode::WorkerManaged,
                v2_compatibility: GatewayV2CompatibilityMode::RemoteSingleWorker,
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
                remote_workers: Some(vec![GatewayRemoteWorkerHealth {
                    worker_id: 0,
                    websocket_url: "ws://127.0.0.1:8081".to_string(),
                    account_id: None,
                    account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    account_capacity_last_changed_at: None,
                    healthy: false,
                    reconnecting: true,
                    reconnect_attempt_count: 2,
                    last_error: Some("remote app server event stream ended".to_string()),
                    last_state_change_at: Some(1710000000),
                    last_error_at: Some(1710000001),
                    next_reconnect_at: Some(1710000002),
                    reconnect_backoff_remaining_seconds: Some(1),
                }]),
                remote_account_labels_complete: Some(true),
                remote_unlabeled_account_worker_count: Some(0),
                remote_unlabeled_account_worker_ids: Some(Vec::new()),
                remote_unlabeled_account_workers: Some(Vec::new()),
                project_worker_routes: Some(Vec::new()),
                worker_pool: None,
            }
        }

        fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
            let (events, _rx) = broadcast::channel(1);
            events.subscribe()
        }

        fn event_visible_to(
            &self,
            _context: &GatewayRequestContext,
            _event: &GatewayEvent,
        ) -> bool {
            true
        }
    }

    let app = router(
        Arc::new(RemoteHealthRuntime),
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
        r#"{"status":"degraded","runtimeMode":"remote","executionMode":"workerManaged","v2Compatibility":"remoteSingleWorker","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":10,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64,"maxPendingClientRequests":64},"v2Connections":{"activeConnectionCount":0,"activeConnectionPendingClientRequestCount":0,"activeConnectionMaxPendingClientRequestCount":0,"activeConnectionPeakPendingClientRequestCount":0,"activeConnectionPendingClientRequestStartedAt":null,"activeConnectionPendingClientRequestWorkerCounts":[],"activeConnectionPendingClientRequestMethodCounts":[],"activeConnectionPendingServerRequestCount":0,"activeConnectionAnsweredButUnresolvedServerRequestCount":0,"activeConnectionServerRequestBacklogCount":0,"activeConnectionMaxServerRequestBacklogCount":0,"activeConnectionPeakServerRequestBacklogCount":0,"activeConnectionServerRequestBacklogStartedAt":null,"activeConnectionServerRequestBacklogWorkerCounts":[],"activeConnectionServerRequestBacklogMethodCounts":[],"projectWorkerRouteSelectionCount":0,"projectWorkerRouteSelectionWorkerCounts":[],"lastProjectWorkerRouteSelectedWorkerId":null,"lastProjectWorkerRouteSelectedTenantId":null,"lastProjectWorkerRouteSelectedProjectId":null,"lastProjectWorkerRouteSelectedThreadId":null,"lastProjectWorkerRouteSelectedAccountId":null,"lastProjectWorkerRouteSelectedAt":null,"accountCapacityEventCounts":{},"accountCapacityEventWorkerCounts":[],"lastAccountCapacityEvent":null,"lastAccountCapacityEventWorkerId":null,"lastAccountCapacityEventTenantId":null,"lastAccountCapacityEventProjectId":null,"lastAccountCapacityEventReason":null,"lastAccountCapacityEventAt":null,"workerReconnectEventCounts":{},"workerReconnectEventWorkerCounts":[],"lastWorkerReconnectEvent":null,"lastWorkerReconnectEventWorkerId":null,"lastWorkerReconnectEventAt":null,"requestCounts":[],"lastRequestMethod":null,"lastRequestOutcome":null,"lastRequestDurationMs":null,"maxRequestDurationMs":null,"lastRequestAt":null,"clientRequestRejectionCounts":[],"lastClientRequestRejectionMethod":null,"lastClientRequestRejectionReason":null,"lastClientRequestRejectionAt":null,"serverRequestRejectionCounts":[],"lastServerRequestRejectionMethod":null,"lastServerRequestRejectionReason":null,"lastServerRequestRejectionAt":null,"serverRequestLifecycleEventCounts":[],"lastServerRequestLifecycleEvent":null,"lastServerRequestLifecycleMethod":null,"lastServerRequestLifecycleAt":null,"failClosedRequestCounts":[],"lastFailClosedRequestMethod":null,"lastFailClosedRequestReconnectBackoffActive":null,"lastFailClosedRequestAt":null,"upstreamRequestFailureCounts":[],"lastUpstreamRequestFailureMethod":null,"lastUpstreamRequestFailureReconnectBackoffActive":null,"lastUpstreamRequestFailureAt":null,"downstreamBackpressureCounts":[],"lastDownstreamBackpressureWorkerId":null,"lastDownstreamBackpressureAt":null,"clientSendTimeoutCount":0,"lastClientSendTimeoutAt":null,"threadListDeduplicationCounts":[],"lastThreadListDeduplicationSelectedWorkerId":null,"lastThreadListDeduplicationAt":null,"threadRouteRecoveryCounts":[],"lastThreadRouteRecoveryOutcome":null,"lastThreadRouteRecoveryAt":null,"degradedThreadDiscoveryCounts":[],"lastDegradedThreadDiscoveryMethod":null,"lastDegradedThreadDiscoveryReconnectBackoffActive":null,"lastDegradedThreadDiscoveryAt":null,"forwardedNotificationCounts":[],"lastForwardedNotificationMethod":null,"lastForwardedNotificationAt":null,"notificationSendFailureCounts":[],"lastNotificationSendFailureMethod":null,"lastNotificationSendFailureOutcome":null,"lastNotificationSendFailureAt":null,"clientResponseSendFailureCounts":[],"lastClientResponseSendFailureMethod":null,"lastClientResponseSendFailureOutcome":null,"lastClientResponseSendFailureAt":null,"downstreamShutdownFailureCounts":[],"lastDownstreamShutdownFailureOutcome":null,"lastDownstreamShutdownFailureAt":null,"closeFrameSendFailureCounts":[],"lastCloseFrameSendFailureCode":null,"lastCloseFrameSendFailureOutcome":null,"lastCloseFrameSendFailureAt":null,"serverRequestForwardSendFailureCounts":[],"lastServerRequestForwardSendFailureMethod":null,"lastServerRequestForwardSendFailureOutcome":null,"lastServerRequestForwardSendFailureAt":null,"serverRequestAnswerDeliveryFailureCounts":[],"lastServerRequestAnswerDeliveryFailureResponseKind":null,"lastServerRequestAnswerDeliveryFailureAt":null,"serverRequestRejectionDeliveryFailureCounts":[],"lastServerRequestRejectionDeliveryFailureMethod":null,"lastServerRequestRejectionDeliveryFailureAt":null,"suppressedNotificationCounts":[],"lastSuppressedNotificationMethod":null,"lastSuppressedNotificationReason":null,"lastSuppressedNotificationAt":null,"protocolViolationCounts":[],"protocolViolationWorkerCounts":[],"lastProtocolViolationPhase":null,"lastProtocolViolationReason":null,"lastProtocolViolationWorkerId":null,"lastProtocolViolationAt":null,"connectionOutcomeCounts":[],"peakActiveConnectionCount":0,"totalConnectionCount":0,"lastConnectionStartedAt":null,"lastConnectionCompletedAt":null,"lastConnectionDurationMs":null,"maxConnectionDurationMs":null,"lastConnectionOutcome":null,"lastConnectionDetail":null,"lastConnectionPendingClientRequestCount":0,"lastConnectionMaxPendingClientRequestCount":0,"lastConnectionPendingClientRequestStartedAt":null,"lastConnectionPendingClientRequestWorkerCounts":[],"lastConnectionPendingClientRequestMethodCounts":[],"lastConnectionPendingServerRequestCount":0,"lastConnectionAnsweredButUnresolvedServerRequestCount":0,"lastConnectionServerRequestBacklogCount":0,"lastConnectionMaxServerRequestBacklogCount":0,"lastConnectionServerRequestBacklogStartedAt":null,"lastConnectionServerRequestBacklogWorkerCounts":[],"lastConnectionServerRequestBacklogMethodCounts":[]},"pendingServerRequestCount":0,"pendingServerRequestKindCounts":{},"pendingServerRequestRouteCounts":[],"pendingServerRequestOldestAt":null,"remoteWorkers":[{"workerId":0,"websocketUrl":"ws://127.0.0.1:8081","accountId":null,"accountCapacity":"available","accountCapacityReason":null,"accountCapacityLastChangedAt":null,"healthy":false,"reconnecting":true,"reconnectAttemptCount":2,"lastError":"remote app server event stream ended","lastStateChangeAt":1710000000,"lastErrorAt":1710000001,"nextReconnectAt":1710000002,"reconnectBackoffRemainingSeconds":1}],"remoteAccountLabelsComplete":true,"remoteUnlabeledAccountWorkerCount":0,"remoteUnlabeledAccountWorkerIds":[],"remoteUnlabeledAccountWorkers":[],"projectWorkerRoutes":[]}"#
    );
}

#[tokio::test]
async fn health_route_serializes_worker_pool_snapshot() {
    struct RemoteHealthRuntime;

    #[async_trait]
    impl GatewayRuntime for RemoteHealthRuntime {
        async fn create_thread(
            &self,
            _context: GatewayRequestContext,
            _request: CreateThreadRequest,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("health-only runtime should not receive create_thread")
        }

        async fn list_threads(
            &self,
            _context: GatewayRequestContext,
            _request: ListThreadsRequest,
        ) -> Result<ListThreadsResponse, GatewayError> {
            unreachable!("health-only runtime should not receive list_threads")
        }

        async fn read_thread(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("health-only runtime should not receive read_thread")
        }

        async fn start_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _request: StartTurnRequest,
        ) -> Result<TurnResponse, GatewayError> {
            unreachable!("health-only runtime should not receive start_turn")
        }

        async fn interrupt_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _turn_id: String,
        ) -> Result<InterruptTurnResponse, GatewayError> {
            unreachable!("health-only runtime should not receive interrupt_turn")
        }

        async fn resolve_server_request(
            &self,
            _context: GatewayRequestContext,
            _request: ResolveServerRequestRequest,
        ) -> Result<ResolveServerRequestResponse, GatewayError> {
            unreachable!("health-only runtime should not receive resolve_server_request")
        }

        fn health(&self) -> GatewayHealthResponse {
            GatewayHealthResponse {
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
                remote_workers: Some(Vec::new()),
                remote_account_labels_complete: Some(true),
                remote_unlabeled_account_worker_count: Some(0),
                remote_unlabeled_account_worker_ids: Some(Vec::new()),
                remote_unlabeled_account_workers: Some(Vec::new()),
                project_worker_routes: Some(Vec::new()),
                worker_pool: Some(crate::api::GatewayWorkerPoolSnapshot {
                    account_count: 1,
                    available_account_count: 0,
                    leased_account_count: 1,
                    cooldown_account_count: 0,
                    policy_eligible_account_count: 1,
                    policy_ineligible_account_count: 0,
                    worker_slot_count: 2,
                    bound_worker_slot_count: 1,
                    healthy_worker_slot_count: 1,
                    unhealthy_worker_slot_count: 1,
                    reconnecting_worker_slot_count: 1,
                    account_login_state_path_count: 1,
                    worker_account_login_state_path_count: 1,
                    pool_account_login_state_path_count: 0,
                    accounts: vec![crate::api::GatewayAccountPoolEntry {
                        account_id: "acct-a".to_string(),
                        account_login_state_path: Some("/codex-home/acct-a".to_string()),
                        lease_state: crate::api::GatewayAccountLeaseState::Leased,
                        leased_worker_id: Some(0),
                        project_route_count: 2,
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                        account_capacity_reason: None,
                        policy_eligible: true,
                        policy_ineligibility_reason: None,
                        cooldown_reason: None,
                        last_error: None,
                    }],
                    worker_slots: vec![
                        crate::api::GatewayWorkerPoolSlot {
                            worker_id: 0,
                            websocket_url: "ws://127.0.0.1:8081".to_string(),
                            account_id: Some("acct-a".to_string()),
                            account_login_state_path: Some("/codex-home/acct-a".to_string()),
                            account_capacity: Some(
                                crate::api::GatewayAccountCapacityStatus::Available,
                            ),
                            account_capacity_reason: None,
                            healthy: Some(true),
                            reconnecting: Some(false),
                            last_error: None,
                        },
                        crate::api::GatewayWorkerPoolSlot {
                            worker_id: 1,
                            websocket_url: "ws://127.0.0.1:8082".to_string(),
                            account_id: None,
                            account_login_state_path: None,
                            account_capacity: None,
                            account_capacity_reason: None,
                            healthy: Some(false),
                            reconnecting: Some(true),
                            last_error: Some("remote app server event stream ended".to_string()),
                        },
                    ],
                }),
            }
        }

        fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
            let (events, _rx) = broadcast::channel(1);
            events.subscribe()
        }

        fn event_visible_to(
            &self,
            _context: &GatewayRequestContext,
            _event: &GatewayEvent,
        ) -> bool {
            true
        }
    }

    let app = router(
        Arc::new(RemoteHealthRuntime),
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

    assert_eq!(
        body["workerPool"],
        serde_json::json!({
            "accountCount": 1,
            "availableAccountCount": 0,
            "leasedAccountCount": 1,
            "cooldownAccountCount": 0,
            "policyEligibleAccountCount": 1,
            "policyIneligibleAccountCount": 0,
            "workerSlotCount": 2,
            "boundWorkerSlotCount": 1,
            "healthyWorkerSlotCount": 1,
            "unhealthyWorkerSlotCount": 1,
            "reconnectingWorkerSlotCount": 1,
            "accountLoginStatePathCount": 1,
            "workerAccountLoginStatePathCount": 1,
            "poolAccountLoginStatePathCount": 0,
            "accounts": [{
                "accountId": "acct-a",
                "accountLoginStatePath": "/codex-home/acct-a",
                "leaseState": "leased",
                "leasedWorkerId": 0,
                "projectRouteCount": 2,
                "accountCapacity": "available",
                "accountCapacityReason": null,
                "policyEligible": true,
                "policyIneligibilityReason": null,
                "cooldownReason": null,
                "lastError": null,
            }],
            "workerSlots": [
                {
                    "workerId": 0,
                    "websocketUrl": "ws://127.0.0.1:8081",
                    "accountId": "acct-a",
                    "accountLoginStatePath": "/codex-home/acct-a",
                    "accountCapacity": "available",
                    "accountCapacityReason": null,
                    "healthy": true,
                    "reconnecting": false,
                    "lastError": null,
                },
                {
                    "workerId": 1,
                    "websocketUrl": "ws://127.0.0.1:8082",
                    "accountId": null,
                    "accountLoginStatePath": null,
                    "accountCapacity": null,
                    "accountCapacityReason": null,
                    "healthy": false,
                    "reconnecting": true,
                    "lastError": "remote app server event stream ended",
                },
            ],
        })
    );
}
