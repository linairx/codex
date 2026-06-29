use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn create_thread_route_returns_thread_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&CreateThreadRequest {
                        cwd: Some("/tmp/project".to_string()),
                        model: None,
                        ephemeral: None,
                    })
                    .expect("request body"),
                ))
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
        r#"{"thread":{"id":"thread-1","preview":"/tmp/project","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":1,"status":{"type":"active","activeFlags":["waitingOnUserInput"]}}}"#
    );
}

#[tokio::test]
async fn list_threads_route_returns_page_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/threads?limit=20&sortKey=updatedAt&sortDirection=desc&searchTerm=gateway")
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
        r#"{"data":[{"id":"thread-1","preview":"gateway","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":2,"status":{"type":"idle"}}],"nextCursor":"cursor-2","backwardsCursor":"cursor-0"}"#
    );
}

#[tokio::test]
async fn read_thread_route_returns_thread_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/threads/thread-123")
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
        r#"{"thread":{"id":"thread-123","preview":"preview","ephemeral":false,"modelProvider":"openai","createdAt":1,"updatedAt":1,"status":{"type":"idle"}}}"#
    );
}

#[tokio::test]
async fn start_turn_route_returns_turn_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-123/turns")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&StartTurnRequest {
                        input: "hello".to_string(),
                    })
                    .expect("request body"),
                ))
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
        r#"{"turn":{"id":"turn-1","status":"inProgress","startedAt":1,"completedAt":null,"durationMs":null,"errorMessage":"hello"}}"#
    );
}

#[tokio::test]
async fn start_turn_route_propagates_fail_closed_account_capacity_error() {
    let app = router(
            Arc::new(FakeRuntime {
                start_turn_error: Some(
                    "thread thread-123 is pinned to worker 1 with exhausted account capacity for turn/start"
                        .to_string(),
                ),
                ..FakeRuntime::default()
            }),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

    let response = app
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

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8(body.to_vec()).expect("utf8"),
        r#"{"error":"thread thread-123 is pinned to worker 1 with exhausted account capacity for turn/start"}"#
    );
}

#[tokio::test]
async fn interrupt_turn_route_returns_accepted_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-123/turns/turn-456/interrupt")
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
        r#"{"status":"accepted"}"#
    );
}

#[tokio::test]
async fn interrupt_turn_route_propagates_fail_closed_account_capacity_error() {
    let app = router(
            Arc::new(FakeRuntime {
                interrupt_turn_error: Some(
                    "thread thread-123 is pinned to worker 1 with exhausted account capacity for turn/interrupt"
                        .to_string(),
                ),
                ..FakeRuntime::default()
            }),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/threads/thread-123/turns/turn-456/interrupt")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8(body.to_vec()).expect("utf8"),
        r#"{"error":"thread thread-123 is pinned to worker 1 with exhausted account capacity for turn/interrupt"}"#
    );
}

#[tokio::test]
async fn resolve_server_request_route_returns_accepted_payload() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::default(),
    );

    let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/server-requests/respond")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"requestId":"req-1","type":"toolRequestUserInput","answers":{"confirm":{"answers":["Yes"]}}}"#,
                    ))
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
        r#"{"status":"accepted"}"#
    );
}

#[tokio::test]
async fn resolve_server_request_route_propagates_fail_closed_account_capacity_error() {
    let app = router(
            Arc::new(FakeRuntime {
                resolve_server_request_error: Some(
                    "thread thread-123 is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
                        .to_string(),
                ),
                ..FakeRuntime::default()
            }),
            GatewayAuth::Disabled,
            GatewayAdmissionController::default(),
        );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/server-requests/respond")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"requestId":"req-1","type":"toolRequestUserInput","answers":{}}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(
        String::from_utf8(body.to_vec()).expect("utf8"),
        r#"{"error":"thread thread-123 is pinned to worker 1 with exhausted account capacity for serverRequest/respond"}"#
    );
}
