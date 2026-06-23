use super::*;
use crate::northbound::v2::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION;
use crate::northbound::v2::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_logs_pending_command_exec_when_client_send_times_out() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let observed_observability = observability.clone();
    let logs = capture_logs_async(async move {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let downstream_addr = listener.local_addr().expect("listener address");
        let (request_observed_tx, request_observed_rx) = oneshot::channel();
        let (flood_started_tx, flood_started_rx) = oneshot::channel();
        let (release_downstream_tx, mut release_downstream_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket should accept");
            let (mut write, mut read) = websocket.split();

            expect_remote_initialize_split(&mut write, &mut read).await;

            let Message::Text(text) = read
                .next()
                .await
                .expect("command/exec request should exist")
                .expect("command/exec request should decode")
            else {
                panic!("expected command/exec request text frame");
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("command/exec request should decode")
            else {
                panic!("expected command/exec request");
            };
            assert_eq!(request.method, "command/exec");
            assert_eq!(
                request.id,
                RequestId::String("command-exec-timeout".to_string())
            );
            request_observed_tx
                .send(())
                .expect("request observation should send");

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
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: observability.clone(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
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

        let mut websocket = connect_websocket_with_small_receive_buffer(addr).await;
        send_initialize(&mut websocket).await;

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("command-exec-timeout".to_string()),
            "command/exec",
            pending_command_exec_params("proc-pending-timeout"),
        )
        .await;
        request_observed_rx
            .await
            .expect("command/exec should reach downstream");
        let active_health = observability.v2_connection_health().snapshot();
        assert_eq!(active_health.active_connection_count, 1);
        assert_eq!(
            active_health.active_connection_pending_client_request_count,
            1
        );
        assert_eq!(
            active_health.active_connection_max_pending_client_request_count,
            1
        );
        assert_eq!(
            active_health.active_connection_peak_pending_client_request_count,
            1
        );
        assert_eq!(
            active_health
                .active_connection_pending_client_request_started_at
                .is_some(),
            true
        );
        assert_eq!(
            active_health.active_connection_pending_client_request_worker_counts,
            vec![crate::api::GatewayV2PendingClientRequestWorkerCounts {
                worker_id: Some(0),
                pending_client_request_count: 1,
            }]
        );
        assert_eq!(
            active_health.active_connection_pending_client_request_method_counts,
            vec![crate::api::GatewayV2PendingClientRequestMethodCounts {
                method: "command/exec".to_string(),
                pending_client_request_count: 1,
            }]
        );
        timeout(Duration::from_secs(5), flood_started_rx)
            .await
            .expect("downstream flood should start")
            .expect("flood start observation should complete");

        timeout(Duration::from_secs(15), async {
            loop {
                let health = observability.v2_connection_health().snapshot();
                if health.last_connection_outcome.as_deref() == Some("client_send_timed_out") {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("slow-client connection should settle");

        let _ = release_downstream_tx.send(());
        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "closing gateway v2 connection because sending to the northbound client timed out"
    ));
    assert!(logs.contains("pending_client_request_count=1"));
    assert!(logs.contains("pending_client_request_ids=[String(\"command-exec-timeout\")]"));
    assert!(logs.contains("pending_client_request_methods=[\"command/exec\"]"));
    assert!(logs.contains("pending_client_request_worker_ids=[0]"));
    assert!(logs.contains("pending_client_request_worker_websocket_urls=[\"ws://127.0.0.1:"));
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(logs.contains("outcome=\"client_send_timed_out\""), "{logs}");
    assert!(
        logs.contains("max_pending_client_request_count=1"),
        "{logs}"
    );
    assert!(
        logs.contains("pending_client_request_worker_counts=["),
        "{logs}"
    );
    assert!(logs.contains("worker_id: Some(0)"), "{logs}");
    assert!(
        logs.contains("pending_client_request_method_counts=["),
        "{logs}"
    );
    assert!(logs.contains("method: \"command/exec\""), "{logs}");

    let settled_health = observed_observability.v2_connection_health().snapshot();
    assert_eq!(settled_health.client_send_timeout_count, 1);
    assert_eq!(settled_health.active_connection_count, 0);
    assert_eq!(
        settled_health.last_connection_outcome.as_deref(),
        Some("client_send_timed_out")
    );
    assert_eq!(
        settled_health.last_connection_pending_client_request_count,
        1
    );
    assert_eq!(
        settled_health.last_connection_max_pending_client_request_count,
        1
    );
    assert_eq!(
        settled_health
            .last_connection_pending_client_request_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health.last_connection_pending_client_request_worker_counts,
        vec![crate::api::GatewayV2PendingClientRequestWorkerCounts {
            worker_id: Some(0),
            pending_client_request_count: 1,
        }]
    );
    assert_eq!(
        settled_health.last_connection_pending_client_request_method_counts,
        vec![crate::api::GatewayV2PendingClientRequestMethodCounts {
            method: "command/exec".to_string(),
            pending_client_request_count: 1,
        }]
    );
    assert_eq!(
        settled_health.last_notification_send_failure_method,
        Some("warning".to_string())
    );
    assert_eq!(
        settled_health.last_notification_send_failure_outcome,
        Some("client_send_timed_out".to_string())
    );
    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("command/exec", "client_send_timed_out", 1),
        ],
    );
}
