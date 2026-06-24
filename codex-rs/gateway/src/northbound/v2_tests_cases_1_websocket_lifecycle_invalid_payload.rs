use super::*;
use crate::northbound::v2::PENDING_SERVER_REQUEST_ABORTED_MESSAGE;
use crate::northbound::v2_wire::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_rejects_pending_server_requests_when_client_sends_invalid_payload() {
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
        assert_eq!(error.error.code, -32601);
        assert_eq!(error.error.message, PENDING_SERVER_REQUEST_ABORTED_MESSAGE);
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
        .send(Message::Text("not json".to_string().into()))
        .await
        .expect("invalid payload should send");

    let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::PROTOCOL
    );
    assert!(
        close_frame
            .reason
            .starts_with(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_scope_and_pending_request_ids_when_client_sends_invalid_payload() {
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
        assert_eq!(error.error.code, -32601);
        assert_eq!(error.error.message, PENDING_SERVER_REQUEST_ABORTED_MESSAGE);
    });

    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
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
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::PROTOCOL
        );
        assert!(
            close_frame
                .reason
                .starts_with(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=0"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(
        logs.contains("thread_scoped_pending_server_request_ids=[String(\"server-request-1\")]")
    );
    assert!(logs.contains("connection_scoped_pending_server_request_ids=[]"));
    assert!(logs.contains("worker_ids=[0]"));
}

#[tokio::test]
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_client_sends_invalid_payload() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (response_observed_tx, response_observed_rx) = oneshot::channel();
    let (release_downstream_tx, release_downstream_rx) = oneshot::channel();
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

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("resolved response should exist")
            .expect("resolved response should decode")
        else {
            panic!("expected resolved response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("resolved response should decode")
        else {
            panic!("expected resolved response");
        };
        assert_eq!(
            response.id,
            RequestId::String("server-request-1".to_string())
        );
        response_observed_tx
            .send(())
            .expect("response observation should send");

        let _ = release_downstream_rx.await;
    });

    let initialize_response = test_initialize_response().await;
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

    let logs = capture_logs_async(async move {
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

        let JSONRPCMessage::Request(first_request) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected forwarded server request");
        };
        assert_eq!(
            first_request.id,
            RequestId::String("server-request-1".to_string())
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: first_request.id,
                    result: serde_json::json!({
                        "contentItems": [{
                            "type": "inputText",
                            "text": "tool output",
                        }],
                        "success": true,
                    }),
                }))
                .expect("resolved response should serialize")
                .into(),
            ))
            .await
            .expect("resolved response should send");

        timeout(Duration::from_secs(5), response_observed_rx)
            .await
            .expect("downstream should observe the resolved response")
            .expect("response observation should complete");

        websocket
            .send(Message::Text("not json".to_string().into()))
            .await
            .expect("invalid payload should send");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::PROTOCOL
        );
        assert!(
            close_frame
                .reason
                .starts_with(INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
        );

        release_downstream_tx
            .send(())
            .expect("downstream release should send");
        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("pending_server_request_count=0"));
    assert!(logs.contains("pending_server_request_ids=[]"));
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
    assert!(logs.contains("connection_outcome"));
}
