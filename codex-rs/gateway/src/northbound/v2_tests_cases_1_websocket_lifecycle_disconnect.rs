use super::*;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::PENDING_SERVER_REQUEST_ABORTED_MESSAGE;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_rejects_pending_server_requests_when_client_disconnects() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (rejection_observed_tx, rejection_observed_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-1".to_string()),
                    method: "item/commandExecution/requestApproval".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "itemId": "item-visible-1",
                        "startedAtMs": 0,
                        "cwd": "/tmp",
                        "reason": "Need approval 1",
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
            .expect("server request rejection should exist")
            .expect("server request rejection should decode")
        else {
            panic!("expected server request rejection text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&text).expect("server request rejection should decode")
        else {
            panic!("expected server request rejection");
        };
        assert_eq!(error.id, RequestId::String("server-request-1".to_string()));
        assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
        assert_eq!(error.error.message, PENDING_SERVER_REQUEST_ABORTED_MESSAGE);
        rejection_observed_tx
            .send(())
            .expect("rejection observation should send");
    });

    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
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

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let JSONRPCMessage::Request(first_request) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected forwarded server request");
        };
        assert_eq!(
            first_request.id,
            RequestId::String("server-request-1".to_string())
        );
        assert_eq!(
            first_request.method,
            "item/commandExecution/requestApproval"
        );

        websocket
            .close(None)
            .await
            .expect("client close should send");

        timeout(Duration::from_secs(5), rejection_observed_rx)
            .await
            .expect("pending server request should be rejected after disconnect")
            .expect("rejection observation should complete");

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=0"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(
        logs.contains("thread_scoped_pending_server_request_ids=[String(\"server-request-1\")]")
    );
    assert!(logs.contains("connection_scoped_pending_server_request_ids=[]"));
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "downstream_server_request_forwarded",
                "item/commandExecution/requestApproval",
                1,
            ),
            (
                "client_cleanup_rejected_thread_scoped",
                "item/commandExecution/requestApproval",
                1,
            ),
            (
                "client_cleanup_rejection_delivered",
                "item/commandExecution/requestApproval",
                1,
            ),
        ],
    );
}

#[tokio::test]
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_client_disconnects() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (rejection_observed_tx, rejection_observed_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-1".to_string()),
                    method: "item/tool/call".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "callId": "call-visible-1",
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

        let Message::Text(dynamic_tool_response_text) = websocket
            .next()
            .await
            .expect("dynamic tool response should exist")
            .expect("dynamic tool response should decode")
        else {
            panic!("expected dynamic tool response text frame");
        };
        let JSONRPCMessage::Response(dynamic_tool_response) =
            serde_json::from_str(&dynamic_tool_response_text)
                .expect("dynamic tool response should decode")
        else {
            panic!("expected dynamic tool response");
        };
        assert_eq!(
            dynamic_tool_response.id,
            RequestId::String("server-request-1".to_string())
        );
        assert_eq!(
            dynamic_tool_response.result,
            serde_json::json!({
                "contentItems": [{
                    "type": "inputText",
                    "text": "tool output",
                }],
                "success": true,
            })
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("server-request-2".to_string()),
                    method: "item/commandExecution/requestApproval".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-visible",
                        "turnId": "turn-visible",
                        "itemId": "item-visible-2",
                        "startedAtMs": 0,
                        "cwd": "/tmp",
                        "reason": "Need approval 2",
                        "command": "pwd",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("pending server request should send");

        let Message::Text(rejection_text) = websocket
            .next()
            .await
            .expect("server request rejection should exist")
            .expect("server request rejection should decode")
        else {
            panic!("expected server request rejection text frame");
        };
        let JSONRPCMessage::Error(error) =
            serde_json::from_str(&rejection_text).expect("server request rejection should decode")
        else {
            panic!("expected server request rejection");
        };
        assert_eq!(error.id, RequestId::String("server-request-2".to_string()));
        assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
        assert_eq!(error.error.message, PENDING_SERVER_REQUEST_ABORTED_MESSAGE);
        rejection_observed_tx
            .send(())
            .expect("rejection observation should send");
    });

    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
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

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let JSONRPCMessage::Request(dynamic_tool_request) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected forwarded dynamic tool request");
        };
        assert_eq!(
            dynamic_tool_request.id,
            RequestId::String("server-request-1".to_string())
        );
        assert_eq!(dynamic_tool_request.method, "item/tool/call");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: dynamic_tool_request.id,
                    result: serde_json::json!({
                        "contentItems": [{
                            "type": "inputText",
                            "text": "tool output",
                        }],
                        "success": true,
                    }),
                }))
                .expect("dynamic tool response should serialize")
                .into(),
            ))
            .await
            .expect("dynamic tool response should send");

        let JSONRPCMessage::Request(pending_request) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected forwarded pending request");
        };
        assert_eq!(
            pending_request.id,
            RequestId::String("server-request-2".to_string())
        );
        assert_eq!(
            pending_request.method,
            "item/commandExecution/requestApproval"
        );

        websocket
            .close(None)
            .await
            .expect("client close should send");

        timeout(Duration::from_secs(5), rejection_observed_rx)
            .await
            .expect("pending server request should be rejected after disconnect")
            .expect("rejection observation should complete");

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-2\")]"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(
        logs.contains("answered_but_unresolved_gateway_request_ids=[String(\"server-request-1\")]")
    );
    assert!(
        logs.contains(
            "answered_but_unresolved_downstream_request_ids=[String(\"server-request-1\")]"
        )
    );
    assert!(logs.contains("answered_but_unresolved_worker_ids=[0]"));
}
