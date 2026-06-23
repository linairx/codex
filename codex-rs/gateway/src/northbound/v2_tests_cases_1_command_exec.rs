use super::*;
use crate::northbound::v2::DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON;
use crate::northbound::v2::INTERNAL_ERROR_CODE;
use crate::northbound::v2_limits::RATE_LIMITED_ERROR_CODE;
use crate::northbound::v2_limits::TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_command_exec_output_notifications() {
    let notification =
        ServerNotification::CommandExecOutputDelta(CommandExecOutputDeltaNotification {
            process_id: "proc-visible".to_string(),
            stream: CommandExecOutputStream::Stdout,
            delta_base64: "AQID".to_string(),
            cap_reached: false,
        });
    let expected =
        tagged_type_to_notification(&notification).expect("notification should serialize");
    let websocket_url = start_mock_remote_server_for_notification(notification).await;
    let (addr, server_task) = spawn_remote_gateway_v2_test_server(
        websocket_url,
        Arc::new(GatewayScopeRegistry::default()),
    )
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize_with_capabilities(
        &mut websocket,
        Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: None,
        }),
    )
    .await;

    let JSONRPCMessage::Notification(actual) = read_websocket_message(&mut websocket).await else {
        panic!("expected command/exec/outputDelta notification");
    };
    assert_eq!(actual, expected);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_command_exec_above_pending_client_limit_without_closing() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let v2_connection_health = observability.v2_connection_health();
    let (first_request_observed_tx, first_request_observed_rx) = oneshot::channel();
    let (finish_first_request_tx, finish_first_request_rx) = oneshot::channel();
    let websocket_url = start_mock_remote_server_for_pending_command_exec(
        first_request_observed_tx,
        finish_first_request_rx,
    )
    .await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability,
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
        timeouts: GatewayV2Timeouts {
            max_pending_server_requests: 4,
            max_pending_client_requests: 1,
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let logs = capture_logs_async(async {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("command-exec-1".to_string()),
            "command/exec",
            pending_command_exec_params("proc-pending-1"),
        )
        .await;
        first_request_observed_rx
            .await
            .expect("first command/exec should reach downstream");
        assert_eq!(
            v2_connection_health
                .snapshot()
                .active_connection_pending_client_request_count,
            1
        );

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("command-exec-2".to_string()),
            "command/exec",
            pending_command_exec_params("proc-pending-2"),
        )
        .await;

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected rate-limit error for second pending command/exec");
        };
        assert_eq!(error.id, RequestId::String("command-exec-2".to_string()));
        assert_eq!(error.error.code, RATE_LIMITED_ERROR_CODE);
        assert_eq!(
            error.error.message,
            TOO_MANY_PENDING_CLIENT_REQUESTS_MESSAGE
        );

        finish_first_request_tx
            .send(())
            .expect("first command/exec completion signal should send");
        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected first command/exec response after releasing downstream");
        };
        assert_eq!(response.id, RequestId::String("command-exec-1".to_string()));
        assert_eq!(
            response.result,
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            })
        );
        assert_eq!(
            v2_connection_health
                .snapshot()
                .active_connection_pending_client_request_count,
            0
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"rate_limited\""), "{logs}");
    assert_command_exec_pending_client_limit_metrics(&metrics);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_client_reuses_pending_command_exec_request_id() {
    let metrics = in_memory_metrics();
    let (first_request_observed_tx, first_request_observed_rx) = oneshot::channel();
    let (_finish_first_request_tx, finish_first_request_rx) = oneshot::channel();
    let websocket_url = start_mock_remote_server_for_pending_command_exec(
        first_request_observed_tx,
        finish_first_request_rx,
    )
    .await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
        timeouts: GatewayV2Timeouts {
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");
    send_initialize(&mut websocket).await;

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("command-exec-duplicate".to_string()),
        "command/exec",
        pending_command_exec_params("proc-pending-1"),
    )
    .await;
    first_request_observed_rx
        .await
        .expect("first command/exec should reach downstream");

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("command-exec-duplicate".to_string()),
        "command/exec",
        pending_command_exec_params("proc-pending-2"),
    )
    .await;

    let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::PROTOCOL);
    assert_eq!(
        close_frame.reason,
        DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "post_initialize",
        "duplicate_request_id",
        "protocol_violation",
        "command/exec",
        "protocol_violation",
        2,
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_client_reuses_pending_request_id_for_other_method() {
    let metrics = in_memory_metrics();
    let (first_request_observed_tx, first_request_observed_rx) = oneshot::channel();
    let (_finish_first_request_tx, finish_first_request_rx) = oneshot::channel();
    let websocket_url = start_mock_remote_server_for_pending_command_exec(
        first_request_observed_tx,
        finish_first_request_rx,
    )
    .await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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
        timeouts: GatewayV2Timeouts {
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("shared-request-id".to_string()),
            "command/exec",
            pending_command_exec_params("proc-pending-1"),
        )
        .await;
        first_request_observed_rx
            .await
            .expect("first command/exec should reach downstream");

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("shared-request-id".to_string()),
            "model/list",
            serde_json::json!({}),
        )
        .await;

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::PROTOCOL);
        assert_eq!(
            close_frame.reason,
            DUPLICATE_PENDING_CLIENT_REQUEST_CLOSE_REASON
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"model/list\""), "{logs}");
    assert!(logs.contains("outcome=\"protocol_violation\""), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_connection_and_request_metrics(
        &metrics,
        "post_initialize",
        "duplicate_request_id",
        "protocol_violation",
        "model/list",
        "protocol_violation",
        1,
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_records_command_exec_request_when_primary_route_fails_closed() {
    let metrics = in_memory_metrics();
    let primary_worker_url = start_mock_remote_server_that_disconnects_after_initialize().await;
    let secondary_worker_url =
        start_mock_remote_server_that_stays_connected_after_initialize().await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: primary_worker_url,
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
                        websocket_url: secondary_worker_url,
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
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts {
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_secs(1)).await;

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("command-exec-fail-closed".to_string()),
            "command/exec",
            pending_command_exec_params("proc-fail-closed"),
        )
        .await;

        let _ = websocket.next().await;
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(logs.contains("method=\"command/exec\""), "{logs}");
    assert!(logs.contains("outcome=\"internal_error\""), "{logs}");
    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("command/exec", "internal_error", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_records_primary_worker_request_when_route_fails_closed() {
    let metrics = in_memory_metrics();
    let primary_worker_url = start_mock_remote_server_that_disconnects_after_initialize().await;
    let secondary_worker_url =
        start_mock_remote_server_that_stays_connected_after_initialize().await;
    let initialize_response = test_initialize_response().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), true),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                        websocket_url: primary_worker_url,
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
                        websocket_url: secondary_worker_url,
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
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts {
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            ..GatewayV2Timeouts::default()
        },
    })
    .await;

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");
        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        send_jsonrpc_request(
            &mut websocket,
            RequestId::String("config-requirements-fail-closed".to_string()),
            "configRequirements/read",
            serde_json::json!({}),
        )
        .await;

        let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
            panic!("expected primary-worker route failure to return a JSON-RPC error");
        };
        assert_eq!(
            error.id,
            RequestId::String("config-requirements-fail-closed".to_string())
        );
        assert_eq!(error.error.code, INTERNAL_ERROR_CODE);
        assert!(
            error
                .error
                .message
                .contains("gateway upstream request failed: primary worker route is unavailable for configRequirements/read"),
            "unexpected error message: {}",
            error.error.message
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 request completed"), "{logs}");
    assert!(
        logs.contains("method=\"configRequirements/read\""),
        "{logs}"
    );
    assert!(logs.contains("outcome=\"internal_error\""), "{logs}");
    assert_v2_request_metrics(
        &metrics,
        &[
            ("initialize", "ok", 1),
            ("configRequirements/read", "internal_error", 1),
        ],
    );

    server_task.abort();
    let _ = server_task.await;
}
