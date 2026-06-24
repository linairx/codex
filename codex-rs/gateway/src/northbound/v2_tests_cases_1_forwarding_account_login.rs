use super::*;
use pretty_assertions::assert_eq;
use serde_json::Value;

#[tokio::test]
async fn websocket_upgrade_forwards_account_login_and_feedback_requests() {
    let cases = vec![
        (
            "account/login/start",
            "account-login-start",
            serde_json::json!({
                "type": "chatgpt",
            }),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-1",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            "account-login-cancel",
            serde_json::json!({
                "loginId": "login-1",
            }),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "feedback/upload",
            "feedback-upload",
            serde_json::json!({
                "classification": "bug",
                "reason": "gateway feedback request",
                "threadId": "thread-visible",
                "includeLogs": true,
                "extraLogFiles": ["/tmp/rollout.jsonl"],
                "tags": {
                    "turn_id": "turn-1",
                },
            }),
            serde_json::json!({
                "threadId": "feedback-thread-1",
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = params.get("threadId").and_then(Value::as_str) {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                initialize_response,
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("passthrough request should serialize")
                .into(),
            ))
            .await
            .expect("passthrough request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected passthrough response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}
