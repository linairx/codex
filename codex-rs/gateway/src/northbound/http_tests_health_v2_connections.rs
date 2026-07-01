use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn health_route_serializes_v2_client_send_timeout_connection_outcome() {
    struct RemoteTimeoutHealthRuntime;

    #[async_trait]
    impl GatewayRuntime for RemoteTimeoutHealthRuntime {
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
                    client_send_timeout_seconds: 1,
                    reconnect_retry_backoff_seconds: 1,
                    max_pending_server_requests: 64,
                    max_pending_client_requests: 64,
                },
                v2_connections: GatewayV2ConnectionHealth {
                    active_connection_count: 1,
                    active_connection_pending_client_request_count: 2,
                    active_connection_max_pending_client_request_count: 2,
                    active_connection_peak_pending_client_request_count: 4,
                    active_connection_pending_client_request_started_at: Some(1710000001),
                    active_connection_pending_client_request_worker_counts: vec![
                        crate::api::GatewayV2PendingClientRequestWorkerCounts {
                            worker_id: Some(3),
                            pending_client_request_count: 2,
                        },
                    ],
                    active_connection_pending_client_request_method_counts: vec![
                        crate::api::GatewayV2PendingClientRequestMethodCounts {
                            method: "command/exec".to_string(),
                            pending_client_request_count: 2,
                        },
                    ],
                    active_connection_pending_server_request_count: 2,
                    active_connection_answered_but_unresolved_server_request_count: 1,
                    active_connection_server_request_backlog_count: 3,
                    active_connection_max_server_request_backlog_count: 4,
                    active_connection_peak_server_request_backlog_count: 5,
                    active_connection_server_request_backlog_started_at: Some(1710000001),
                    active_connection_server_request_backlog_worker_counts: vec![
                        crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                            worker_id: Some(3),
                            pending_server_request_count: 2,
                            answered_but_unresolved_server_request_count: 1,
                            server_request_backlog_count: 3,
                        },
                    ],
                    active_connection_server_request_backlog_method_counts: vec![
                        crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                            method: "item/tool/requestUserInput".to_string(),
                            pending_server_request_count: 2,
                            answered_but_unresolved_server_request_count: 1,
                            server_request_backlog_count: 3,
                        },
                    ],
                    project_worker_route_selection_count: 3,
                    project_worker_route_selection_worker_counts: vec![
                        crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                            worker_id: 3,
                            project_worker_route_selection_count: 3,
                        },
                    ],
                    last_project_worker_route_selected_worker_id: Some(3),
                    last_project_worker_route_selected_tenant_id: Some("tenant-a".to_string()),
                    last_project_worker_route_selected_project_id: Some("project-a".to_string()),
                    last_project_worker_route_selected_thread_id: Some("thread-a".to_string()),
                    last_project_worker_route_selected_account_id: Some("acct-a".to_string()),
                    last_project_worker_route_selected_at: Some(1710000002),
                    account_capacity_event_counts: std::collections::BTreeMap::from([(
                        "active_thread_handoff_failure".to_string(),
                        2,
                    )]),
                    account_capacity_event_worker_counts: vec![
                        crate::api::GatewayV2AccountCapacityWorkerEventCounts {
                            worker_id: 3,
                            event_counts: std::collections::BTreeMap::from([(
                                "active_thread_handoff_failure".to_string(),
                                2,
                            )]),
                        },
                    ],
                    last_account_capacity_event: Some("active_thread_handoff_failure".to_string()),
                    last_account_capacity_event_worker_id: Some(3),
                    last_account_capacity_event_tenant_id: Some("tenant-a".to_string()),
                    last_account_capacity_event_project_id: Some("project-a".to_string()),
                    last_account_capacity_event_reason: Some(
                        "thread thread-a is pinned to worker 3 with exhausted account capacity"
                            .to_string(),
                    ),
                    last_account_capacity_event_at: Some(1710000002),
                    worker_reconnect_event_counts: std::collections::BTreeMap::from([(
                        "success".to_string(),
                        2,
                    )]),
                    worker_reconnect_event_worker_counts: vec![
                        crate::api::GatewayV2WorkerReconnectWorkerEventCounts {
                            worker_id: 3,
                            event_counts: std::collections::BTreeMap::from([(
                                "success".to_string(),
                                2,
                            )]),
                        },
                    ],
                    last_worker_reconnect_event: Some("success".to_string()),
                    last_worker_reconnect_event_worker_id: Some(3),
                    last_worker_reconnect_event_at: Some(1710000002),
                    request_counts: vec![crate::api::GatewayV2RequestCounts {
                        method: "thread/start".to_string(),
                        outcome: "ok".to_string(),
                        count: 2,
                    }],
                    last_request_method: Some("thread/start".to_string()),
                    last_request_outcome: Some("ok".to_string()),
                    last_request_duration_ms: Some(42),
                    max_request_duration_ms: Some(99),
                    last_request_at: Some(1710000002),
                    client_request_rejection_counts: vec![
                        crate::api::GatewayV2ClientRequestRejectionCounts {
                            method: "command/exec".to_string(),
                            reason: "pending_limit".to_string(),
                            count: 1,
                        },
                    ],
                    last_client_request_rejection_method: Some("command/exec".to_string()),
                    last_client_request_rejection_reason: Some("pending_limit".to_string()),
                    last_client_request_rejection_at: Some(1710000002),
                    server_request_rejection_counts: vec![
                        crate::api::GatewayV2ServerRequestRejectionCounts {
                            method: "item/tool/requestUserInput".to_string(),
                            reason: "pending_limit".to_string(),
                            count: 2,
                        },
                    ],
                    last_server_request_rejection_method: Some(
                        "item/tool/requestUserInput".to_string(),
                    ),
                    last_server_request_rejection_reason: Some("pending_limit".to_string()),
                    last_server_request_rejection_at: Some(1710000002),
                    server_request_lifecycle_event_counts: vec![
                        crate::api::GatewayV2ServerRequestLifecycleEventCounts {
                            event: "client_server_request_delivered".to_string(),
                            method: "response".to_string(),
                            count: 3,
                        },
                    ],
                    last_server_request_lifecycle_event: Some(
                        "client_server_request_delivered".to_string(),
                    ),
                    last_server_request_lifecycle_method: Some("response".to_string()),
                    last_server_request_lifecycle_at: Some(1710000002),
                    fail_closed_request_counts: vec![
                        crate::api::GatewayV2FailClosedRequestCounts {
                            method: "config/read".to_string(),
                            reconnect_backoff_active: true,
                            count: 2,
                        },
                    ],
                    last_fail_closed_request_method: Some("config/read".to_string()),
                    last_fail_closed_request_reconnect_backoff_active: Some(true),
                    last_fail_closed_request_at: Some(1710000002),
                    upstream_request_failure_counts: vec![
                        crate::api::GatewayV2UpstreamRequestFailureCounts {
                            method: "thread/read".to_string(),
                            reconnect_backoff_active: false,
                            count: 1,
                        },
                    ],
                    last_upstream_request_failure_method: Some("thread/read".to_string()),
                    last_upstream_request_failure_reconnect_backoff_active: Some(false),
                    last_upstream_request_failure_at: Some(1710000002),
                    downstream_backpressure_counts: vec![
                        crate::api::GatewayV2DownstreamBackpressureCounts {
                            worker_id: Some(3),
                            count: 2,
                        },
                    ],
                    last_downstream_backpressure_worker_id: Some(3),
                    last_downstream_backpressure_at: Some(1710000002),
                    client_send_timeout_count: 1,
                    last_client_send_timeout_at: Some(1710000002),
                    thread_list_deduplication_counts: vec![
                        crate::api::GatewayV2ThreadListDeduplicationCounts {
                            selected_worker_id: Some(3),
                            count: 2,
                        },
                    ],
                    last_thread_list_deduplication_selected_worker_id: Some(3),
                    last_thread_list_deduplication_at: Some(1710000002),
                    thread_route_recovery_counts: vec![
                        crate::api::GatewayV2ThreadRouteRecoveryCounts {
                            outcome: "miss".to_string(),
                            count: 1,
                        },
                    ],
                    last_thread_route_recovery_outcome: Some("miss".to_string()),
                    last_thread_route_recovery_at: Some(1710000002),
                    degraded_thread_discovery_counts: vec![
                        crate::api::GatewayV2DegradedThreadDiscoveryCounts {
                            method: "thread/list".to_string(),
                            reconnect_backoff_active: true,
                            count: 1,
                        },
                    ],
                    last_degraded_thread_discovery_method: Some("thread/list".to_string()),
                    last_degraded_thread_discovery_reconnect_backoff_active: Some(true),
                    last_degraded_thread_discovery_at: Some(1710000002),
                    forwarded_notification_counts: vec![
                        crate::api::GatewayV2ForwardedNotificationCounts {
                            method: "item/agentMessage/delta".to_string(),
                            count: 3,
                        },
                    ],
                    last_forwarded_notification_method: Some("item/agentMessage/delta".to_string()),
                    last_forwarded_notification_at: Some(1710000002),
                    notification_send_failure_counts: vec![
                        crate::api::GatewayV2NotificationSendFailureCounts {
                            method: "warning".to_string(),
                            outcome: "client_send_timed_out".to_string(),
                            count: 1,
                        },
                    ],
                    last_notification_send_failure_method: Some("warning".to_string()),
                    last_notification_send_failure_outcome: Some(
                        "client_send_timed_out".to_string(),
                    ),
                    last_notification_send_failure_at: Some(1710000002),
                    client_response_send_failure_counts: vec![
                        crate::api::GatewayV2ClientResponseSendFailureCounts {
                            method: "model/list".to_string(),
                            outcome: "client_send_timed_out".to_string(),
                            count: 1,
                        },
                    ],
                    last_client_response_send_failure_method: Some("model/list".to_string()),
                    last_client_response_send_failure_outcome: Some(
                        "client_send_timed_out".to_string(),
                    ),
                    last_client_response_send_failure_at: Some(1710000002),
                    downstream_shutdown_failure_counts: vec![
                        crate::api::GatewayV2DownstreamShutdownFailureCounts {
                            outcome: "client_send_timed_out".to_string(),
                            count: 1,
                        },
                    ],
                    last_downstream_shutdown_failure_outcome: Some(
                        "client_send_timed_out".to_string(),
                    ),
                    last_downstream_shutdown_failure_at: Some(1710000002),
                    close_frame_send_failure_counts: vec![
                        crate::api::GatewayV2CloseFrameSendFailureCounts {
                            code: 1008,
                            outcome: "client_send_timed_out".to_string(),
                            count: 1,
                        },
                    ],
                    last_close_frame_send_failure_code: Some(1008),
                    last_close_frame_send_failure_outcome: Some(
                        "client_send_timed_out".to_string(),
                    ),
                    last_close_frame_send_failure_at: Some(1710000002),
                    server_request_forward_send_failure_counts: vec![
                        crate::api::GatewayV2ServerRequestForwardSendFailureCounts {
                            method: "item/tool/requestUserInput".to_string(),
                            outcome: "client_send_timed_out".to_string(),
                            count: 1,
                        },
                    ],
                    last_server_request_forward_send_failure_method: Some(
                        "item/tool/requestUserInput".to_string(),
                    ),
                    last_server_request_forward_send_failure_outcome: Some(
                        "client_send_timed_out".to_string(),
                    ),
                    last_server_request_forward_send_failure_at: Some(1710000002),
                    server_request_answer_delivery_failure_counts: vec![
                        crate::api::GatewayV2ServerRequestAnswerDeliveryFailureCounts {
                            response_kind: "response".to_string(),
                            count: 1,
                        },
                    ],
                    last_server_request_answer_delivery_failure_response_kind: Some(
                        "response".to_string(),
                    ),
                    last_server_request_answer_delivery_failure_at: Some(1710000002),
                    server_request_rejection_delivery_failure_counts: vec![
                        crate::api::GatewayV2ServerRequestRejectionDeliveryFailureCounts {
                            method: "item/permissions/requestApproval".to_string(),
                            count: 1,
                        },
                    ],
                    last_server_request_rejection_delivery_failure_method: Some(
                        "item/permissions/requestApproval".to_string(),
                    ),
                    last_server_request_rejection_delivery_failure_at: Some(1710000002),
                    suppressed_notification_counts: vec![
                        crate::api::GatewayV2SuppressedNotificationCounts {
                            method: "skills/changed".to_string(),
                            reason: "pending_refresh".to_string(),
                            count: 2,
                        },
                    ],
                    last_suppressed_notification_method: Some("skills/changed".to_string()),
                    last_suppressed_notification_reason: Some("pending_refresh".to_string()),
                    last_suppressed_notification_at: Some(1710000002),
                    protocol_violation_counts: vec![crate::api::GatewayV2ProtocolViolationCounts {
                        phase: "post_initialize".to_string(),
                        reason: "invalid_jsonrpc".to_string(),
                        count: 2,
                    }],
                    protocol_violation_worker_counts: vec![
                        crate::api::GatewayV2ProtocolViolationWorkerCounts {
                            worker_id: 3,
                            violation_counts: vec![crate::api::GatewayV2ProtocolViolationCounts {
                                phase: "downstream".to_string(),
                                reason: "invalid_jsonrpc".to_string(),
                                count: 1,
                            }],
                        },
                    ],
                    last_protocol_violation_phase: Some("downstream".to_string()),
                    last_protocol_violation_reason: Some("invalid_jsonrpc".to_string()),
                    last_protocol_violation_worker_id: Some(3),
                    last_protocol_violation_at: Some(1710000002),
                    connection_outcome_counts: vec![crate::api::GatewayV2ConnectionOutcomeCounts {
                        outcome: "client_send_timed_out".to_string(),
                        count: 2,
                    }],
                    peak_active_connection_count: 3,
                    total_connection_count: 7,
                    last_connection_started_at: Some(1710000001),
                    last_connection_completed_at: Some(1710000003),
                    last_connection_duration_ms: Some(2500),
                    max_connection_duration_ms: Some(4000),
                    last_connection_outcome: Some("client_send_timed_out".to_string()),
                    last_connection_detail: Some("gateway websocket send timed out".to_string()),
                    last_connection_pending_client_request_count: 4,
                    last_connection_max_pending_client_request_count: 6,
                    last_connection_pending_client_request_started_at: Some(1710000002),
                    last_connection_pending_client_request_worker_counts: vec![
                        crate::api::GatewayV2PendingClientRequestWorkerCounts {
                            worker_id: Some(3),
                            pending_client_request_count: 4,
                        },
                    ],
                    last_connection_pending_client_request_method_counts: vec![
                        crate::api::GatewayV2PendingClientRequestMethodCounts {
                            method: "command/exec".to_string(),
                            pending_client_request_count: 4,
                        },
                    ],
                    last_connection_pending_server_request_count: 2,
                    last_connection_answered_but_unresolved_server_request_count: 1,
                    last_connection_server_request_backlog_count: 3,
                    last_connection_max_server_request_backlog_count: 5,
                    last_connection_server_request_backlog_started_at: Some(1710000002),
                    last_connection_server_request_backlog_worker_counts: vec![
                        crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                            worker_id: Some(3),
                            pending_server_request_count: 2,
                            answered_but_unresolved_server_request_count: 1,
                            server_request_backlog_count: 3,
                        },
                    ],
                    last_connection_server_request_backlog_method_counts: vec![
                        crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                            method: "item/tool/requestUserInput".to_string(),
                            pending_server_request_count: 2,
                            answered_but_unresolved_server_request_count: 1,
                            server_request_backlog_count: 3,
                        },
                    ],
                },
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
                    healthy: true,
                    reconnecting: false,
                    reconnect_attempt_count: 0,
                    last_error: None,
                    last_state_change_at: Some(1710000000),
                    last_error_at: None,
                    next_reconnect_at: None,
                    reconnect_backoff_remaining_seconds: None,
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
        Arc::new(RemoteTimeoutHealthRuntime),
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
        r#"{"status":"degraded","runtimeMode":"remote","executionMode":"workerManaged","v2Compatibility":"remoteSingleWorker","v2Transport":{"initializeTimeoutSeconds":30,"clientSendTimeoutSeconds":1,"reconnectRetryBackoffSeconds":1,"maxPendingServerRequests":64,"maxPendingClientRequests":64},"v2Connections":{"activeConnectionCount":1,"activeConnectionPendingClientRequestCount":2,"activeConnectionMaxPendingClientRequestCount":2,"activeConnectionPeakPendingClientRequestCount":4,"activeConnectionPendingClientRequestStartedAt":1710000001,"activeConnectionPendingClientRequestWorkerCounts":[{"workerId":3,"pendingClientRequestCount":2}],"activeConnectionPendingClientRequestMethodCounts":[{"method":"command/exec","pendingClientRequestCount":2}],"activeConnectionPendingServerRequestCount":2,"activeConnectionAnsweredButUnresolvedServerRequestCount":1,"activeConnectionServerRequestBacklogCount":3,"activeConnectionMaxServerRequestBacklogCount":4,"activeConnectionPeakServerRequestBacklogCount":5,"activeConnectionServerRequestBacklogStartedAt":1710000001,"activeConnectionServerRequestBacklogWorkerCounts":[{"workerId":3,"pendingServerRequestCount":2,"answeredButUnresolvedServerRequestCount":1,"serverRequestBacklogCount":3}],"activeConnectionServerRequestBacklogMethodCounts":[{"method":"item/tool/requestUserInput","pendingServerRequestCount":2,"answeredButUnresolvedServerRequestCount":1,"serverRequestBacklogCount":3}],"projectWorkerRouteSelectionCount":3,"projectWorkerRouteSelectionWorkerCounts":[{"workerId":3,"projectWorkerRouteSelectionCount":3}],"lastProjectWorkerRouteSelectedWorkerId":3,"lastProjectWorkerRouteSelectedTenantId":"tenant-a","lastProjectWorkerRouteSelectedProjectId":"project-a","lastProjectWorkerRouteSelectedThreadId":"thread-a","lastProjectWorkerRouteSelectedAccountId":"acct-a","lastProjectWorkerRouteSelectedAt":1710000002,"accountCapacityEventCounts":{"active_thread_handoff_failure":2},"accountCapacityEventWorkerCounts":[{"workerId":3,"eventCounts":{"active_thread_handoff_failure":2}}],"lastAccountCapacityEvent":"active_thread_handoff_failure","lastAccountCapacityEventWorkerId":3,"lastAccountCapacityEventTenantId":"tenant-a","lastAccountCapacityEventProjectId":"project-a","lastAccountCapacityEventReason":"thread thread-a is pinned to worker 3 with exhausted account capacity","lastAccountCapacityEventAt":1710000002,"workerReconnectEventCounts":{"success":2},"workerReconnectEventWorkerCounts":[{"workerId":3,"eventCounts":{"success":2}}],"lastWorkerReconnectEvent":"success","lastWorkerReconnectEventWorkerId":3,"lastWorkerReconnectEventAt":1710000002,"requestCounts":[{"method":"thread/start","outcome":"ok","count":2}],"lastRequestMethod":"thread/start","lastRequestOutcome":"ok","lastRequestDurationMs":42,"maxRequestDurationMs":99,"lastRequestAt":1710000002,"clientRequestRejectionCounts":[{"method":"command/exec","reason":"pending_limit","count":1}],"lastClientRequestRejectionMethod":"command/exec","lastClientRequestRejectionReason":"pending_limit","lastClientRequestRejectionAt":1710000002,"serverRequestRejectionCounts":[{"method":"item/tool/requestUserInput","reason":"pending_limit","count":2}],"lastServerRequestRejectionMethod":"item/tool/requestUserInput","lastServerRequestRejectionReason":"pending_limit","lastServerRequestRejectionAt":1710000002,"serverRequestLifecycleEventCounts":[{"event":"client_server_request_delivered","method":"response","count":3}],"lastServerRequestLifecycleEvent":"client_server_request_delivered","lastServerRequestLifecycleMethod":"response","lastServerRequestLifecycleAt":1710000002,"failClosedRequestCounts":[{"method":"config/read","reconnectBackoffActive":true,"count":2}],"lastFailClosedRequestMethod":"config/read","lastFailClosedRequestReconnectBackoffActive":true,"lastFailClosedRequestAt":1710000002,"upstreamRequestFailureCounts":[{"method":"thread/read","reconnectBackoffActive":false,"count":1}],"lastUpstreamRequestFailureMethod":"thread/read","lastUpstreamRequestFailureReconnectBackoffActive":false,"lastUpstreamRequestFailureAt":1710000002,"downstreamBackpressureCounts":[{"workerId":3,"count":2}],"lastDownstreamBackpressureWorkerId":3,"lastDownstreamBackpressureAt":1710000002,"clientSendTimeoutCount":1,"lastClientSendTimeoutAt":1710000002,"threadListDeduplicationCounts":[{"selectedWorkerId":3,"count":2}],"lastThreadListDeduplicationSelectedWorkerId":3,"lastThreadListDeduplicationAt":1710000002,"threadRouteRecoveryCounts":[{"outcome":"miss","count":1}],"lastThreadRouteRecoveryOutcome":"miss","lastThreadRouteRecoveryAt":1710000002,"degradedThreadDiscoveryCounts":[{"method":"thread/list","reconnectBackoffActive":true,"count":1}],"lastDegradedThreadDiscoveryMethod":"thread/list","lastDegradedThreadDiscoveryReconnectBackoffActive":true,"lastDegradedThreadDiscoveryAt":1710000002,"forwardedNotificationCounts":[{"method":"item/agentMessage/delta","count":3}],"lastForwardedNotificationMethod":"item/agentMessage/delta","lastForwardedNotificationAt":1710000002,"notificationSendFailureCounts":[{"method":"warning","outcome":"client_send_timed_out","count":1}],"lastNotificationSendFailureMethod":"warning","lastNotificationSendFailureOutcome":"client_send_timed_out","lastNotificationSendFailureAt":1710000002,"clientResponseSendFailureCounts":[{"method":"model/list","outcome":"client_send_timed_out","count":1}],"lastClientResponseSendFailureMethod":"model/list","lastClientResponseSendFailureOutcome":"client_send_timed_out","lastClientResponseSendFailureAt":1710000002,"downstreamShutdownFailureCounts":[{"outcome":"client_send_timed_out","count":1}],"lastDownstreamShutdownFailureOutcome":"client_send_timed_out","lastDownstreamShutdownFailureAt":1710000002,"closeFrameSendFailureCounts":[{"code":1008,"outcome":"client_send_timed_out","count":1}],"lastCloseFrameSendFailureCode":1008,"lastCloseFrameSendFailureOutcome":"client_send_timed_out","lastCloseFrameSendFailureAt":1710000002,"serverRequestForwardSendFailureCounts":[{"method":"item/tool/requestUserInput","outcome":"client_send_timed_out","count":1}],"lastServerRequestForwardSendFailureMethod":"item/tool/requestUserInput","lastServerRequestForwardSendFailureOutcome":"client_send_timed_out","lastServerRequestForwardSendFailureAt":1710000002,"serverRequestAnswerDeliveryFailureCounts":[{"responseKind":"response","count":1}],"lastServerRequestAnswerDeliveryFailureResponseKind":"response","lastServerRequestAnswerDeliveryFailureAt":1710000002,"serverRequestRejectionDeliveryFailureCounts":[{"method":"item/permissions/requestApproval","count":1}],"lastServerRequestRejectionDeliveryFailureMethod":"item/permissions/requestApproval","lastServerRequestRejectionDeliveryFailureAt":1710000002,"suppressedNotificationCounts":[{"method":"skills/changed","reason":"pending_refresh","count":2}],"lastSuppressedNotificationMethod":"skills/changed","lastSuppressedNotificationReason":"pending_refresh","lastSuppressedNotificationAt":1710000002,"protocolViolationCounts":[{"phase":"post_initialize","reason":"invalid_jsonrpc","count":2}],"protocolViolationWorkerCounts":[{"workerId":3,"violationCounts":[{"phase":"downstream","reason":"invalid_jsonrpc","count":1}]}],"lastProtocolViolationPhase":"downstream","lastProtocolViolationReason":"invalid_jsonrpc","lastProtocolViolationWorkerId":3,"lastProtocolViolationAt":1710000002,"connectionOutcomeCounts":[{"outcome":"client_send_timed_out","count":2}],"peakActiveConnectionCount":3,"totalConnectionCount":7,"lastConnectionStartedAt":1710000001,"lastConnectionCompletedAt":1710000003,"lastConnectionDurationMs":2500,"maxConnectionDurationMs":4000,"lastConnectionOutcome":"client_send_timed_out","lastConnectionDetail":"gateway websocket send timed out","lastConnectionPendingClientRequestCount":4,"lastConnectionMaxPendingClientRequestCount":6,"lastConnectionPendingClientRequestStartedAt":1710000002,"lastConnectionPendingClientRequestWorkerCounts":[{"workerId":3,"pendingClientRequestCount":4}],"lastConnectionPendingClientRequestMethodCounts":[{"method":"command/exec","pendingClientRequestCount":4}],"lastConnectionPendingServerRequestCount":2,"lastConnectionAnsweredButUnresolvedServerRequestCount":1,"lastConnectionServerRequestBacklogCount":3,"lastConnectionMaxServerRequestBacklogCount":5,"lastConnectionServerRequestBacklogStartedAt":1710000002,"lastConnectionServerRequestBacklogWorkerCounts":[{"workerId":3,"pendingServerRequestCount":2,"answeredButUnresolvedServerRequestCount":1,"serverRequestBacklogCount":3}],"lastConnectionServerRequestBacklogMethodCounts":[{"method":"item/tool/requestUserInput","pendingServerRequestCount":2,"answeredButUnresolvedServerRequestCount":1,"serverRequestBacklogCount":3}]},"pendingServerRequestCount":0,"pendingServerRequestKindCounts":{},"pendingServerRequestRouteCounts":[],"pendingServerRequestOldestAt":null,"remoteWorkers":[{"workerId":0,"websocketUrl":"ws://127.0.0.1:8081","accountId":null,"accountCapacity":"available","accountCapacityReason":null,"accountCapacityLastChangedAt":null,"healthy":true,"reconnecting":false,"reconnectAttemptCount":0,"lastError":null,"lastStateChangeAt":1710000000,"lastErrorAt":null,"nextReconnectAt":null,"reconnectBackoffRemainingSeconds":null}],"remoteAccountLabelsComplete":true,"remoteUnlabeledAccountWorkerCount":0,"remoteUnlabeledAccountWorkerIds":[],"remoteUnlabeledAccountWorkers":[],"projectWorkerRoutes":[]}"#
    );
}
