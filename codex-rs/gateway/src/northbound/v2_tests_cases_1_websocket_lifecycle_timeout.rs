use super::*;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION;
use crate::northbound::v2::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION;
use crate::northbound::v2::PENDING_SERVER_REQUEST_ABORTED_MESSAGE;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_logs_and_rejects_pending_server_requests_when_client_send_times_out() {
    let metrics = in_memory_metrics();
    let observed_metrics = metrics.clone();
    let logs = capture_logs_async(async move {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let downstream_addr = listener.local_addr().expect("listener address");
        let (rejection_observed_tx, rejection_observed_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");
            let (mut write, mut read) = websocket.split();

            expect_remote_initialize_split(&mut write, &mut read).await;

            write
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

            sleep(Duration::from_millis(50)).await;

            let large_warning_payload =
                serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                    method: "warning".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": null,
                        "message": "w".repeat(1024 * 1024),
                    })),
                }))
                .expect("warning notification should serialize");

            let writer = tokio::spawn(async move {
                for _ in 0..256 {
                    if write
                        .send(Message::Text(large_warning_payload.clone().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            loop {
                let Some(frame) = read.next().await else {
                    break;
                };
                let frame = frame.expect("incoming downstream frame should decode");
                let Message::Text(text) = frame else {
                    continue;
                };
                let JSONRPCMessage::Error(error) =
                    serde_json::from_str(&text).expect("downstream message should decode")
                else {
                    continue;
                };
                if error.id == RequestId::String("server-request-1".to_string()) {
                    assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
                    assert_eq!(error.error.message, PENDING_SERVER_REQUEST_ABORTED_MESSAGE);
                    rejection_observed_tx
                        .send(())
                        .expect("rejection observation should send");
                    break;
                }
            }

            let _ = writer.await;
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
            observability: GatewayObservability::new(Some(metrics), false),
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
            timeouts: GatewayV2Timeouts {
                initialize: Duration::from_secs(30),
                client_send: Duration::from_millis(1),
                reconnect_retry_backoff: Duration::from_secs(1),
                max_pending_server_requests: MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
                max_pending_client_requests: MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
            },
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

        timeout(Duration::from_secs(5), rejection_observed_rx)
            .await
            .expect("pending server request should be rejected after timeout")
            .expect("rejection observation should complete");

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "closing gateway v2 connection because sending to the northbound client timed out"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("client_send_timed_out"));
    assert_v2_client_send_timeout_and_server_request_lifecycle_metrics(
        &observed_metrics,
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
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_client_send_times_out() {
    let metrics = in_memory_metrics();
    let observed_metrics = metrics.clone();
    let logs = capture_logs_async(async move {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let downstream_addr = listener.local_addr().expect("listener address");
        let (flood_started_tx, flood_started_rx) = oneshot::channel();
        let (release_downstream_tx, mut release_downstream_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");
            let (mut write, mut read) = websocket.split();

            expect_remote_initialize_split(&mut write, &mut read).await;

            write
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

            let Message::Text(text) = read
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

            sleep(Duration::from_millis(50)).await;

            let large_warning_payload =
                serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                    method: "warning".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": null,
                        "message": "w".repeat(1024 * 1024),
                    })),
                }))
                .expect("warning notification should serialize");

            flood_started_tx
                .send(())
                .expect("flood start observation should send");

            loop {
                tokio::select! {
                    _ = &mut release_downstream_rx => break,
                    result = write.send(Message::Text(large_warning_payload.clone().into())) => {
                        if result.is_err() {
                            break;
                        }
                    }
                }
            }
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
            observability: GatewayObservability::new(Some(metrics), false),
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
            timeouts: GatewayV2Timeouts {
                initialize: Duration::from_secs(30),
                client_send: Duration::from_millis(1),
                reconnect_retry_backoff: Duration::from_secs(1),
                max_pending_server_requests: MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
                max_pending_client_requests: MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
            },
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
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: first_request.id,
                    result: serde_json::json!({ "approved": true }),
                }))
                .expect("resolved response should serialize")
                .into(),
            ))
            .await
            .expect("resolved response should send");

        timeout(Duration::from_secs(5), flood_started_rx)
            .await
            .expect("downstream flood should start")
            .expect("flood start observation should complete");

        sleep(Duration::from_millis(500)).await;
        let _ = release_downstream_tx.send(());

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "closing gateway v2 connection because sending to the northbound client timed out"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("connection_detail=\"gateway websocket send timed out\""));
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
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("client_send_timed_out"));
    assert_v2_client_send_timeout_and_server_request_lifecycle_metrics(
        &observed_metrics,
        &[
            (
                "downstream_server_request_forwarded",
                "item/commandExecution/requestApproval",
                1,
            ),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivered", "response", 1),
            (
                "client_cleanup_answered_but_unresolved",
                "item/commandExecution/requestApproval",
                1,
            ),
        ],
    );
}
