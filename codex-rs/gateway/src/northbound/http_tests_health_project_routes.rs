use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn health_route_serializes_project_worker_routes() {
    struct ProjectRouteHealthRuntime;

    #[async_trait]
    impl GatewayRuntime for ProjectRouteHealthRuntime {
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
                remote_workers: Some(vec![GatewayRemoteWorkerHealth {
                    worker_id: 3,
                    websocket_url: "ws://127.0.0.1:8083".to_string(),
                    account_id: Some("acct-c".to_string()),
                    account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                    account_capacity_reason: None,
                    account_capacity_last_changed_at: Some(1710000004),
                    healthy: true,
                    reconnecting: false,
                    reconnect_attempt_count: 0,
                    last_error: None,
                    last_state_change_at: Some(1710000004),
                    last_error_at: None,
                    next_reconnect_at: None,
                    reconnect_backoff_remaining_seconds: None,
                }]),
                remote_account_labels_complete: Some(true),
                remote_unlabeled_account_worker_count: Some(0),
                remote_unlabeled_account_worker_ids: Some(Vec::new()),
                remote_unlabeled_account_workers: Some(Vec::new()),
                project_worker_routes: Some(vec![crate::api::GatewayProjectWorkerRoute {
                    tenant_id: "tenant-a".to_string(),
                    project_id: "project-a".to_string(),
                    worker_id: 3,
                    account_id: Some("acct-c".to_string()),
                    account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                    worker_healthy: true,
                    account_routing_eligible: true,
                }]),
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
        Arc::new(ProjectRouteHealthRuntime),
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
        body["remoteWorkers"],
        serde_json::json!([{
            "workerId": 3,
            "websocketUrl": "ws://127.0.0.1:8083",
            "accountId": "acct-c",
            "accountCapacity": "available",
            "accountCapacityReason": null,
            "accountCapacityLastChangedAt": 1710000004,
            "healthy": true,
            "reconnecting": false,
            "reconnectAttemptCount": 0,
            "lastError": null,
            "lastStateChangeAt": 1710000004,
            "lastErrorAt": null,
            "nextReconnectAt": null,
            "reconnectBackoffRemainingSeconds": null,
        }])
    );
    assert_eq!(
        body["projectWorkerRoutes"],
        serde_json::json!([{
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 3,
            "accountId": "acct-c",
            "accountCapacity": "available",
            "workerHealthy": true,
            "accountRoutingEligible": true,
        }])
    );
}

#[tokio::test]
async fn health_route_serializes_mixed_project_worker_routes() {
    struct ProjectRouteHealthRuntime;

    #[async_trait]
    impl GatewayRuntime for ProjectRouteHealthRuntime {
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
                        worker_id: 3,
                        websocket_url: "ws://127.0.0.1:8083".to_string(),
                        account_id: Some("acct-c".to_string()),
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                        account_capacity_reason: None,
                        account_capacity_last_changed_at: Some(1710000004),
                        healthy: true,
                        reconnecting: false,
                        reconnect_attempt_count: 0,
                        last_error: None,
                        last_state_change_at: Some(1710000004),
                        last_error_at: None,
                        next_reconnect_at: None,
                        reconnect_backoff_remaining_seconds: None,
                    },
                    GatewayRemoteWorkerHealth {
                        worker_id: 4,
                        websocket_url: "ws://127.0.0.1:8084".to_string(),
                        account_id: Some("acct-d".to_string()),
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
                        account_capacity_reason: Some("quota exhausted".to_string()),
                        account_capacity_last_changed_at: Some(1710000005),
                        healthy: false,
                        reconnecting: true,
                        reconnect_attempt_count: 1,
                        last_error: Some("remote app server disconnected".to_string()),
                        last_state_change_at: Some(1710000005),
                        last_error_at: Some(1710000005),
                        next_reconnect_at: Some(1710000006),
                        reconnect_backoff_remaining_seconds: Some(1),
                    },
                ]),
                remote_account_labels_complete: Some(true),
                remote_unlabeled_account_worker_count: Some(0),
                remote_unlabeled_account_worker_ids: Some(Vec::new()),
                remote_unlabeled_account_workers: Some(Vec::new()),
                project_worker_routes: Some(vec![
                    crate::api::GatewayProjectWorkerRoute {
                        tenant_id: "tenant-a".to_string(),
                        project_id: "project-a".to_string(),
                        worker_id: 3,
                        account_id: Some("acct-c".to_string()),
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                        worker_healthy: true,
                        account_routing_eligible: true,
                    },
                    crate::api::GatewayProjectWorkerRoute {
                        tenant_id: "tenant-a".to_string(),
                        project_id: "project-b".to_string(),
                        worker_id: 4,
                        account_id: Some("acct-d".to_string()),
                        account_capacity: crate::api::GatewayAccountCapacityStatus::Exhausted,
                        worker_healthy: false,
                        account_routing_eligible: false,
                    },
                ]),
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
        Arc::new(ProjectRouteHealthRuntime),
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
        body["remoteWorkers"],
        serde_json::json!([{
            "workerId": 3,
            "websocketUrl": "ws://127.0.0.1:8083",
            "accountId": "acct-c",
            "accountCapacity": "available",
            "accountCapacityReason": null,
            "accountCapacityLastChangedAt": 1710000004,
            "healthy": true,
            "reconnecting": false,
            "reconnectAttemptCount": 0,
            "lastError": null,
            "lastStateChangeAt": 1710000004,
            "lastErrorAt": null,
            "nextReconnectAt": null,
            "reconnectBackoffRemainingSeconds": null,
        }, {
            "workerId": 4,
            "websocketUrl": "ws://127.0.0.1:8084",
            "accountId": "acct-d",
            "accountCapacity": "exhausted",
            "accountCapacityReason": "quota exhausted",
            "accountCapacityLastChangedAt": 1710000005,
            "healthy": false,
            "reconnecting": true,
            "reconnectAttemptCount": 1,
            "lastError": "remote app server disconnected",
            "lastStateChangeAt": 1710000005,
            "lastErrorAt": 1710000005,
            "nextReconnectAt": 1710000006,
            "reconnectBackoffRemainingSeconds": 1,
        }])
    );
    assert_eq!(
        body["projectWorkerRoutes"],
        serde_json::json!([{
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 3,
            "accountId": "acct-c",
            "accountCapacity": "available",
            "workerHealthy": true,
            "accountRoutingEligible": true,
        }, {
            "tenantId": "tenant-a",
            "projectId": "project-b",
            "workerId": 4,
            "accountId": "acct-d",
            "accountCapacity": "exhausted",
            "workerHealthy": false,
            "accountRoutingEligible": false,
        }])
    );
}
