use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_turn_start() {
    let expected_result = serde_json::json!({
        "turn": {
            "id": "turn-reconnected",
        },
    });
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::clone(&scope_registry),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_a,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: worker_b,
                        auth_token: None,
                    },
                    client_name: "codex-gateway".to_string(),
                    client_version: "0.0.0-test".to_string(),
                    experimental_api: false,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: 4,
                },
            ],
            test_initialize_response().await,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("turn-start".to_string()),
                method: "turn/start".to_string(),
                params: Some(serde_json::json!({
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "collaborationMode": null,
                    "input": [],
                    "cwd": "/tmp/worker-b",
                    "effort": null,
                    "model": "gpt-5",
                    "outputSchema": null,
                    "personality": null,
                    "responsesapiClientMetadata": null,
                    "sandboxPolicy": null,
                    "summary": null,
                    "threadId": "thread-worker-b",
                })),
                trace: None,
            }))
            .expect("turn/start request should serialize")
            .into(),
        ))
        .await
        .expect("turn/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("turn/start response should arrive") else {
        panic!("expected turn/start response");
    };
    assert_eq!(response.id, RequestId::String("turn-start".to_string()));
    assert_eq!(response.result, expected_result);

    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["turn/start".to_string()]
    );

    server_task.abort();
    let _ = server_task.await;
}
