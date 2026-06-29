use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn v1_routes_require_bearer_token_when_configured() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::BearerToken {
            token: "secret-token".to_string(),
        },
        GatewayAdmissionController::default(),
    );

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), StatusCode::UNAUTHORIZED);

    let authorized_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .header("authorization", "Bearer secret-token")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(authorized_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn healthz_route_stays_public_when_auth_is_enabled() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::BearerToken {
            token: "secret-token".to_string(),
        },
        GatewayAdmissionController::default(),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn runtime_errors_map_to_http_status_codes() {
    #[derive(Default)]
    struct ErrorRuntime;

    #[async_trait]
    impl GatewayRuntime for ErrorRuntime {
        async fn create_thread(
            &self,
            _context: GatewayRequestContext,
            _request: CreateThreadRequest,
        ) -> Result<ThreadResponse, GatewayError> {
            Err(GatewayError::InvalidRequest("bad create".to_string()))
        }

        async fn list_threads(
            &self,
            _context: GatewayRequestContext,
            _request: ListThreadsRequest,
        ) -> Result<ListThreadsResponse, GatewayError> {
            Err(GatewayError::InvalidRequest("bad list".to_string()))
        }

        async fn read_thread(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
        ) -> Result<ThreadResponse, GatewayError> {
            Err(GatewayError::NotFound(
                "thread not found: thread-404".to_string(),
            ))
        }

        async fn start_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _request: StartTurnRequest,
        ) -> Result<TurnResponse, GatewayError> {
            Err(GatewayError::Upstream("upstream unavailable".to_string()))
        }

        async fn interrupt_turn(
            &self,
            _context: GatewayRequestContext,
            _thread_id: String,
            _turn_id: String,
        ) -> Result<InterruptTurnResponse, GatewayError> {
            Err(GatewayError::NotFound(
                "turn not found: turn-404".to_string(),
            ))
        }

        async fn resolve_server_request(
            &self,
            _context: GatewayRequestContext,
            _request: ResolveServerRequestRequest,
        ) -> Result<ResolveServerRequestResponse, GatewayError> {
            Err(GatewayError::NotFound(
                "server request not found: req-404".to_string(),
            ))
        }

        fn health(&self) -> GatewayHealthResponse {
            GatewayHealthResponse {
                status: GatewayHealthStatus::Unavailable,
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
        Arc::new(ErrorRuntime),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"cwd":"/tmp/project"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(create_response.status(), StatusCode::BAD_REQUEST);

    let read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/threads/thread-404")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(read_response.status(), StatusCode::NOT_FOUND);

    let turn_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-123/turns")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"hello"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(turn_response.status(), StatusCode::BAD_GATEWAY);

    let interrupt_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-123/turns/turn-404/interrupt")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(interrupt_response.status(), StatusCode::NOT_FOUND);

    let resolve_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/server-requests/respond")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"requestId":"req-404","type":"toolRequestUserInput","answers":{}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resolve_response.status(), StatusCode::NOT_FOUND);
}
