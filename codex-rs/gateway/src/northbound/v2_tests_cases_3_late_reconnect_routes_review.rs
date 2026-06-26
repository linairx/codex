use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_review_start_and_backfills_review_thread_route()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b =
        start_mock_remote_server_for_disconnect_then_reconnectable_review_start_then_thread_read()
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
                id: RequestId::String("review-start".to_string()),
                method: "review/start".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "target": {
                        "type": "custom",
                        "instructions": "Review the current change",
                    },
                    "delivery": "detached",
                })),
                trace: None,
            }))
            .expect("review/start request should serialize")
            .into(),
        ))
        .await
        .expect("review/start request should send");

    let JSONRPCMessage::Response(review_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("review/start response should arrive") else {
        panic!("expected review/start response");
    };
    assert_eq!(
        review_response.id,
        RequestId::String("review-start".to_string())
    );
    assert_eq!(
        review_response.result,
        serde_json::json!({
            "turn": {
                "id": "turn-review",
                "items": [],
                "status": "pending",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null,
            },
            "reviewThreadId": "thread-review",
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-read-review".to_string()),
                method: "thread/read".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-review",
                    "includeTurns": false,
                })),
                trace: None,
            }))
            .expect("thread/read request should serialize")
            .into(),
        ))
        .await
        .expect("thread/read request should send");

    let JSONRPCMessage::Response(thread_read_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("thread/read response should arrive") else {
        panic!("expected thread/read response");
    };
    assert_eq!(
        thread_read_response.id,
        RequestId::String("thread-read-review".to_string())
    );
    assert_eq!(
        thread_read_response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-review",
                "name": "Detached review thread",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    server_task.abort();
    let _ = server_task.await;
}
