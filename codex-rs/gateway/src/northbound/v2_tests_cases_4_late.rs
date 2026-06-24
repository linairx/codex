use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_4_late_cleanup_tail.rs"]
mod v2_tests_cases_4_late_cleanup_tail;

#[tokio::test]
async fn websocket_upgrade_drops_duplicate_multi_worker_server_request_resolved_notifications() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async(async {
        let listener_a = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let worker_a_addr = listener_a.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener_a.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            let request = read_websocket_request(&mut websocket).await;
            assert_eq!(request.method, "model/list");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({
                            "data": [],
                            "nextCursor": null,
                        }),
                    }))
                    .expect("worker A model/list response should serialize")
                    .into(),
                ))
                .await
                .expect("worker A model/list response should send");
        });

        let listener_b = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let worker_b_addr = listener_b.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener_b.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");

            expect_remote_initialize(&mut websocket).await;

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                        id: RequestId::String("worker-request-1".to_string()),
                        method: "item/commandExecution/requestApproval".to_string(),
                        params: Some(serde_json::json!({
                            "threadId": "thread-visible",
                            "turnId": "turn-visible",
                            "itemId": "item-visible",
                            "startedAtMs": 0,
                            "cwd": "/tmp",
                            "reason": "Need approval",
                            "command": "pwd",
                        })),
                        trace: None,
                    }))
                    .expect("server request should serialize")
                    .into(),
                ))
                .await
                .expect("server request should send");

            let Message::Text(text) = websocket
                .next()
                .await
                .expect("server request response should exist")
                .expect("server request response should decode")
            else {
                panic!("expected server request response text frame");
            };
            let JSONRPCMessage::Response(response) =
                serde_json::from_str(&text).expect("server request response should decode")
            else {
                panic!("expected server request response");
            };
            assert_eq!(
                response.id,
                RequestId::String("worker-request-1".to_string())
            );
            assert_eq!(response.result, serde_json::json!({ "approved": true }));

            for _ in 0..2 {
                send_remote_notification(
                    &mut websocket,
                    "serverRequest/resolved",
                    serde_json::json!({
                        "threadId": "thread-visible",
                        "requestId": "worker-request-1",
                    }),
                )
                .await;
            }

            let request = read_websocket_request(&mut websocket).await;
            assert_eq!(request.method, "model/list");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({
                            "data": [],
                            "nextCursor": null,
                        }),
                    }))
                    .expect("worker B model/list response should serialize")
                    .into(),
                ))
                .await
                .expect("worker B model/list response should send");
        });

        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            },
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: format!("ws://{worker_a_addr}"),
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
                            websocket_url: format!("ws://{worker_b_addr}"),
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

        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-visible".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-visible".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let JSONRPCMessage::Request(forwarded_request) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected forwarded server request");
        };
        assert_eq!(
            forwarded_request.method,
            "item/commandExecution/requestApproval"
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: forwarded_request.id.clone(),
                    result: serde_json::json!({ "approved": true }),
                }))
                .expect("server request approval should serialize")
                .into(),
            ))
            .await
            .expect("server request approval should send");

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            "serverRequest/resolved",
            serde_json::json!({
                "threadId": "thread-visible",
                "requestId": forwarded_request.id,
            }),
        );

        let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(duplicate.is_err(), true);

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("model-list".to_string()),
                    method: "model/list".to_string(),
                    params: Some(serde_json::json!({
                        "cursor": null,
                        "limit": null,
                        "includeHidden": true,
                    })),
                    trace: None,
                }))
                .expect("model/list request should serialize")
                .into(),
            ))
            .await
            .expect("model/list request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected model/list response");
        };
        assert_eq!(response.id, RequestId::String("model-list".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            })
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "dropping duplicate downstream serverRequest/resolved replay after request-id translation"
    ));
    assert!(!logs.contains(
        "suppressing downstream notification for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert!(logs.contains("downstream_request_id=String(\"worker-request-1\")"));
    assert!(logs.contains("remaining_resolved_route_count=0"));
    assert!(logs.contains("remaining_resolved_gateway_request_ids=[]"));
    assert!(logs.contains("remaining_resolved_downstream_request_ids=[]"));
    assert!(logs.contains("remaining_resolved_worker_ids=[]"));
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "downstream_server_request_forwarded",
                "item/commandExecution/requestApproval",
                1,
            ),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivered", "response", 1),
            (
                "downstream_server_request_resolved",
                "serverRequest/resolved",
                1,
            ),
            ("duplicate_resolved_replay", "serverRequest/resolved", 1),
        ],
    );
}

#[tokio::test]
async fn websocket_upgrade_filters_thread_loaded_list_responses_by_scope() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/loaded/list",
        serde_json::json!({
            "cursor": null,
            "limit": 10,
        }),
        serde_json::json!({
            "data": ["thread-visible", "thread-hidden"],
            "nextCursor": null,
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
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
                id: RequestId::String("thread-loaded-list".to_string()),
                method: "thread/loaded/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 10,
                })),
                trace: None,
            }))
            .expect("thread loaded list request should serialize")
            .into(),
        ))
        .await
        .expect("thread loaded list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread loaded list response");
    };
    assert_eq!(
        response.id,
        RequestId::String("thread-loaded-list".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": ["thread-visible"],
            "nextCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_hidden_downstream_server_requests() {
    let metrics = in_memory_metrics();
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_hidden_server_request(
        JSONRPCRequest {
            id: RequestId::String("hidden-server-request".to_string()),
            method: "item/commandExecution/requestApproval".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-hidden",
                "turnId": "turn-hidden",
                "itemId": "item-hidden",
                "startedAtMs": 0,
                "cwd": "/tmp",
                "reason": "Need to run a hidden command",
                "command": "pwd",
            })),
            trace: None,
        },
        "thread not found",
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        },
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
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

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let hidden_request = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(hidden_request.is_err(), true);
    })
    .await;

    assert_v2_server_request_rejection_and_lifecycle_metrics(
        &metrics,
        "item/commandExecution/requestApproval",
        "hidden_thread",
        &[
            (
                "downstream_server_request_rejected_hidden_thread",
                "item/commandExecution/requestApproval",
                1,
            ),
            (
                "downstream_server_request_rejection_delivered",
                "item/commandExecution/requestApproval",
                1,
            ),
        ],
    );
    assert!(logs.contains(
        "rejecting downstream server request for a thread outside the gateway request scope"
    ));
    assert!(logs.contains("tenant-a"), "{logs}");
    assert!(logs.contains("project-a"), "{logs}");
    assert!(logs.contains("worker_websocket_url=\"ws://"), "{logs}");
    assert!(
        logs.contains("request_id=String(\"hidden-server-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("item/commandExecution/requestApproval"),
        "{logs}"
    );
    assert!(logs.contains("thread-hidden"));

    server_task.abort();
    let _ = server_task.await;
}
