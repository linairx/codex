use super::*;
use axum::http::header;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn openai_account_routes_manage_current_account() {
    let app = router(
        Arc::new(FakeRuntime::default()),
        GatewayAuth::Disabled,
        GatewayAdmissionController::new(GatewayAdmissionConfig::default()),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/auth/openai/account?refreshToken=true")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).expect("body should parse"),
        serde_json::json!({
            "account": { "type": "apiKey" },
            "requiresOpenaiAuth": true
        })
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/openai/account")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"apiKey":"sk-test"}"#))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/auth/openai/account")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn openai_login_redirect_uses_configured_callback_port() {
    let runtime = Arc::new(FakeRuntime::default());
    let callback_ports = runtime.openai_login_callback_ports.clone();
    let app = router(
        runtime,
        GatewayAuth::Disabled,
        GatewayAdmissionController::new(GatewayAdmissionConfig::default()),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/openai/callback-port")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"callbackPort":1455}"#))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/auth/openai/login")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("https://chatgpt.com/auth")
    );
    assert_eq!(
        *callback_ports
            .lock()
            .expect("callback port lock should not be poisoned"),
        vec![Some(1455)]
    );
}
