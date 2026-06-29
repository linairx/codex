use super::*;
use pretty_assertions::assert_eq;
#[test]
fn sse_event_filter_keeps_matching_thread() {
    let event = map_event_to_sse(
        Ok(GatewayEvent {
            method: "turn/completed".to_string(),
            thread_id: Some("thread-123".to_string()),
            data: serde_json::json!({ "turnId": "turn-1" }),
        }),
        Some("thread-123"),
        &FakeRuntime::default(),
        &GatewayRequestContext::default(),
    );

    assert_eq!(event.is_some(), true);
}

#[test]
fn sse_event_filter_drops_non_matching_thread() {
    let event = map_event_to_sse(
        Ok(GatewayEvent {
            method: "turn/completed".to_string(),
            thread_id: Some("thread-123".to_string()),
            data: serde_json::json!({ "turnId": "turn-1" }),
        }),
        Some("thread-999"),
        &FakeRuntime::default(),
        &GatewayRequestContext::default(),
    );

    assert!(event.is_none());
}

#[test]
fn sse_event_converts_broadcast_lag_to_gateway_lag_event() {
    let event = map_event_to_sse(
        Err(BroadcastStreamRecvError::Lagged(3)),
        None,
        &FakeRuntime::default(),
        &GatewayRequestContext::default(),
    );

    assert_eq!(event.is_some(), true);
}

#[test]
fn sse_event_filter_drops_events_outside_request_scope() {
    #[derive(Default)]
    struct HiddenRuntime;

    #[async_trait]
    impl GatewayRuntime for HiddenRuntime {
        async fn create_thread(
            &self,
            _context: GatewayRequestContext,
            _request: CreateThreadRequest,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("not used")
        }

        async fn list_threads(
            &self,
            _context: GatewayRequestContext,
            _request: ListThreadsRequest,
        ) -> Result<ListThreadsResponse, GatewayError> {
            unreachable!("not used")
        }

        async fn read_thread(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
        ) -> Result<ThreadResponse, GatewayError> {
            unreachable!("not used")
        }

        async fn start_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _request: StartTurnRequest,
        ) -> Result<TurnResponse, GatewayError> {
            unreachable!("not used")
        }

        async fn interrupt_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _turn_id: String,
        ) -> Result<InterruptTurnResponse, GatewayError> {
            unreachable!("not used")
        }

        async fn resolve_server_request(
            &self,
            _context: GatewayRequestContext,
            _request: ResolveServerRequestRequest,
        ) -> Result<ResolveServerRequestResponse, GatewayError> {
            unreachable!("not used")
        }

        fn health(&self) -> GatewayHealthResponse {
            GatewayHealthResponse {
                status: GatewayHealthStatus::Ok,
                runtime_mode: "embedded".to_string(),
                execution_mode: GatewayExecutionMode::InProcess,
                v2_compatibility: GatewayV2CompatibilityMode::Embedded,
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
                remote_workers: None,
                remote_account_labels_complete: None,
                remote_unlabeled_account_worker_count: None,
                remote_unlabeled_account_worker_ids: None,
                remote_unlabeled_account_workers: None,
                project_worker_routes: None,
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
            false
        }
    }

    let event = map_event_to_sse(
        Ok(GatewayEvent {
            method: "turn/completed".to_string(),
            thread_id: Some("thread-123".to_string()),
            data: serde_json::json!({ "turnId": "turn-1" }),
        }),
        Some("thread-123"),
        &HiddenRuntime,
        &GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
    );

    assert!(event.is_none());
}
