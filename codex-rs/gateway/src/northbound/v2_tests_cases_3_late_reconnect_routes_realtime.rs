use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_sticky_realtime_requests() {
    let cases = vec![
        (
            "thread/realtime/start",
            "realtime-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "outputModality": "text",
                "transport": {
                    "type": "websocket"
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendText",
            "realtime-append-text",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "text": "hello realtime",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendAudio",
            "realtime-append-audio",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-visible",
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/stop",
            "realtime-stop",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
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
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky realtime request should serialize")
                .into(),
            ))
            .await
            .expect("sticky realtime request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky realtime response should arrive") else {
            panic!("expected sticky realtime response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}
