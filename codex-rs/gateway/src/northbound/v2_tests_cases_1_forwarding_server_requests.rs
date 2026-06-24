use super::*;
use pretty_assertions::assert_eq;
use tokio::sync::oneshot;

#[tokio::test]
async fn handle_app_server_event_records_server_request_forward_send_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (outcome_tx, mut outcome_rx) = mpsc::channel(1);
    let metrics_for_server = metrics.clone();
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let metrics = metrics_for_server.clone();
            let outcome_tx = outcome_tx.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::new(Some(metrics), false);
                    let scope_registry = Arc::new(GatewayScopeRegistry::default());
                    let request_context = GatewayRequestContext::default();
                    scope_registry
                        .register_thread("thread-visible".to_string(), request_context.clone());
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_millis(1),
                        max_pending_server_requests: 512,
                        max_pending_client_requests: 4,
                        opt_out_notification_methods: HashSet::new(),
                    };
                    let (event_tx, event_rx) = mpsc::channel(1);
                    let mut router = GatewayV2DownstreamRouter {
                        workers: Vec::new(),
                        event_tx,
                        event_rx,
                        shutdown_txs: Vec::new(),
                        event_tasks: Vec::new(),
                        next_worker: 0,
                        initialized_notification_sent: false,
                        active_fs_watches: HashMap::new(),
                        reconnect_retry_after: HashMap::new(),
                        reconnect_state: None,
                    };
                    let session_factory = GatewayV2SessionFactory::remote_single(
                        RemoteAppServerConnectArgs {
                            endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                websocket_url: "ws://worker-a.invalid".to_string(),
                                auth_token: None,
                            },
                            client_name: "codex-gateway".to_string(),
                            client_version: "0.0.0-test".to_string(),
                            experimental_api: false,
                            mcp_server_openai_form_elicitation: false,
                            opt_out_notification_methods: Vec::new(),
                            channel_capacity: 4,
                        },
                        test_initialize_response().await,
                    );
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::new(),
                        skills_changed_pending_refresh: false,
                        forwarded_connection_notifications: HashMap::new(),
                    };

                    for index in 0..256 {
                        let request = ServerRequest::CommandExecutionRequestApproval {
                            request_id: RequestId::String(format!("server-request-{index}")),
                            params: CommandExecutionRequestApprovalParams {
                                thread_id: "thread-visible".to_string(),
                                turn_id: "turn-visible".to_string(),
                                item_id: format!("item-visible-{index}"),
                                approval_id: None,
                                environment_id: None,
                                reason: Some("r".repeat(1024 * 1024)),
                                network_approval_context: None,
                                command: Some("pwd".to_string()),
                                cwd: None,
                                command_actions: None,
                                additional_permissions: None,
                                proposed_execpolicy_amendment: None,
                                proposed_network_policy_amendments: None,
                                available_decisions: None,
                                started_at_ms: 0,
                            },
                        };
                        let result = handle_app_server_event(
                            &mut socket,
                            &mut router,
                            &session_factory,
                            &connection,
                            &mut event_state,
                            &HashMap::new(),
                            DownstreamWorkerEvent {
                                worker_id: None,
                                event: Some(AppServerEvent::ServerRequest(request)),
                            },
                        )
                        .await;
                        if let Err(err) = result {
                            let _ = outcome_tx
                                .send(
                                    crate::northbound::v2::classify_v2_connection_error(&err)
                                        .to_string(),
                                )
                                .await;
                            return;
                        }
                    }

                    let _ = outcome_tx.send("no_send_failure".to_string()).await;
                })
            }
        }),
    );
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server should run");
    });

    let (_websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");
    let outcome = timeout(Duration::from_secs(5), outcome_rx.recv())
        .await
        .expect("server request send should fail")
        .expect("server request send outcome should be recorded");

    assert_eq!(outcome, "client_send_timed_out");
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut saw_lifecycle_event = false;
    let mut saw_forward_send_failure = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_server_request_lifecycle_events" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        for point in sum.data_points() {
                            let attributes: BTreeMap<String, String> = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            if attributes
                                == BTreeMap::from([
                                    (
                                        "event".to_string(),
                                        "downstream_server_request_forward_delivery_failed"
                                            .to_string(),
                                    ),
                                    (
                                        "method".to_string(),
                                        "item/commandExecution/requestApproval".to_string(),
                                    ),
                                ])
                            {
                                assert_eq!(point.value(), 1);
                                saw_lifecycle_event = true;
                            }
                        }
                    }
                    _ => panic!("unexpected server-request lifecycle count aggregation"),
                },
                _ => panic!("unexpected server-request lifecycle count type"),
            },
            "gateway_v2_server_request_forward_send_failures" => match metric.data() {
                AggregatedMetrics::U64(data) => match data {
                    MetricData::Sum(sum) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                (
                                    "method".to_string(),
                                    "item/commandExecution/requestApproval".to_string()
                                ),
                                ("outcome".to_string(), "client_send_timed_out".to_string()),
                            ])
                        );
                        saw_forward_send_failure = true;
                    }
                    _ => {
                        panic!("unexpected server-request forward send failure aggregation")
                    }
                },
                _ => panic!("unexpected server-request forward send failure count type"),
            },
            _ => {}
        }
    }
    assert!(saw_lifecycle_event);
    assert!(saw_forward_send_failure);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_dynamic_tool_call_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-tool-call".to_string()),
                    method: "item/tool/call".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "callId": "call-visible",
                        "tool": "image-edit",
                        "arguments": {
                            "prompt": "Sharpen this image",
                            "strength": 0.5,
                        },
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
            .expect("tool call response should exist")
            .expect("tool call response should decode")
        else {
            panic!("expected tool call response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("tool call response should decode")
        else {
            panic!("expected tool call response");
        };
        assert_eq!(
            response.id,
            RequestId::String("server-request-tool-call".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "contentItems": [{
                    "type": "inputText",
                    "text": "tool output",
                }],
                "success": true,
            })
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext {
            tenant_id: "default".to_string(),
            project_id: None,
        },
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
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

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded dynamic tool call request");
    };
    assert_eq!(
        request.id,
        RequestId::String("server-request-tool-call".to_string())
    );
    assert_eq!(request.method, "item/tool/call");
    assert_json_params_eq(
        request.params,
        Some(serde_json::json!({
            "threadId": "thread-visible",
            "turnId": "turn-visible",
            "callId": "call-visible",
            "tool": "image-edit",
            "arguments": {
                "prompt": "Sharpen this image",
                "strength": 0.5,
            },
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "contentItems": [{
                        "type": "inputText",
                        "text": "tool output",
                    }],
                    "success": true,
                }),
            }))
            .expect("tool call response should serialize")
            .into(),
        ))
        .await
        .expect("tool call response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_records_client_server_request_response_lifecycle() {
    let metrics = in_memory_metrics();
    let (response_observed_tx, response_observed_rx) = oneshot::channel();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-answer".to_string()),
                    method: "item/tool/call".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "callId": "call-visible",
                        "tool": "image-edit",
                        "arguments": {
                            "prompt": "Sharpen this image",
                        },
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
            RequestId::String("server-request-answer".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "contentItems": [{
                    "type": "inputText",
                    "text": "tool output",
                }],
                "success": true,
            })
        );
        send_remote_notification(
            &mut websocket,
            "serverRequest/resolved",
            serde_json::json!({
                "threadId": "thread-visible",
                "requestId": "server-request-answer",
            }),
        )
        .await;
        response_observed_tx
            .send(())
            .expect("response observation should send");
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext {
            tenant_id: "default".to_string(),
            project_id: None,
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
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
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

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded server request");
    };
    assert_eq!(
        request.id,
        RequestId::String("server-request-answer".to_string())
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "contentItems": [{
                        "type": "inputText",
                        "text": "tool output",
                    }],
                    "success": true,
                }),
            }))
            .expect("server request response should serialize")
            .into(),
        ))
        .await
        .expect("server request response should send");

    timeout(Duration::from_secs(5), response_observed_rx)
        .await
        .expect("downstream should observe server request response")
        .expect("response observation should complete");
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "server-request-answer",
        }),
    );
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            ("downstream_server_request_forwarded", "item/tool/call", 1),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivered", "response", 1),
            (
                "downstream_server_request_resolved",
                "serverRequest/resolved",
                1,
            ),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_records_client_server_request_error_lifecycle() {
    let metrics = in_memory_metrics();
    let (error_observed_tx, error_observed_rx) = oneshot::channel();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-reject".to_string()),
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
            .expect("server request error should exist")
            .expect("server request error should decode")
        else {
            panic!("expected server request error text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&text).expect("server request error should decode")
        else {
            panic!("expected server request error");
        };
        assert_eq!(
            error.id,
            RequestId::String("server-request-reject".to_string())
        );
        assert_eq!(error.error.code, -32000);
        assert_eq!(error.error.message, "user rejected");
        send_remote_notification(
            &mut websocket,
            "serverRequest/resolved",
            serde_json::json!({
                "threadId": "thread-visible",
                "requestId": "server-request-reject",
            }),
        )
        .await;
        error_observed_tx
            .send(())
            .expect("error observation should send");
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext {
            tenant_id: "default".to_string(),
            project_id: None,
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
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
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

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded server request");
    };
    assert_eq!(
        request.id,
        RequestId::String("server-request-reject".to_string())
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                id: request.id,
                error: JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                },
            }))
            .expect("server request error should serialize")
            .into(),
        ))
        .await
        .expect("server request error should send");

    timeout(Duration::from_secs(5), error_observed_rx)
        .await
        .expect("downstream should observe server request error")
        .expect("error observation should complete");
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "server-request-reject",
        }),
    );
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "downstream_server_request_forwarded",
                "item/commandExecution/requestApproval",
                1,
            ),
            ("client_server_request_answered", "error", 1),
            ("client_server_request_delivered", "error", 1),
            (
                "downstream_server_request_resolved",
                "serverRequest/resolved",
                1,
            ),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}
