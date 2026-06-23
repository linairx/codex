use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_1_command_exec.rs"]
mod v2_tests_cases_1_command_exec;

#[path = "v2_tests_cases_1_turn_control.rs"]
mod v2_tests_cases_1_turn_control;

#[path = "v2_tests_cases_1_thread_list.rs"]
mod v2_tests_cases_1_thread_list;

#[path = "v2_tests_cases_1_plugin_list.rs"]
mod v2_tests_cases_1_plugin_list;

#[path = "v2_tests_cases_1_realtime.rs"]
mod v2_tests_cases_1_realtime;

#[path = "v2_tests_cases_1_fuzzy_search.rs"]
mod v2_tests_cases_1_fuzzy_search;

#[path = "v2_tests_cases_1_auth.rs"]
mod v2_tests_cases_1_auth;

#[path = "v2_tests_cases_1_late_passthrough.rs"]
mod v2_tests_cases_1_late_passthrough;

#[path = "v2_tests_cases_1_late_timeout.rs"]
mod v2_tests_cases_1_late_timeout;

#[path = "v2_tests_cases_1_late_experimental.rs"]
mod v2_tests_cases_1_late_experimental;

#[tokio::test]
async fn websocket_upgrade_reports_downstream_protocol_violations() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_that_sends_invalid_jsonrpc_after_initialize().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-downstream".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-downstream".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let frame = wait_for_close_frame(&mut websocket).await;
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::ERROR
        );
        assert_eq!(
            close_frame
                .reason
                .starts_with("downstream app-server disconnected:"),
            true
        );
        assert!(close_frame.reason.contains("sent invalid JSON-RPC"));
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"downstream_protocol_violation\""),
        "{logs}"
    );
    assert!(logs.contains("downstream app-server sent a malformed v2 protocol frame"));
    assert!(logs.contains("tenant-downstream"));
    assert!(logs.contains("project-downstream"));
    assert!(logs.contains("worker_websocket_url"));
    assert!(logs.contains("reason=\"invalid_jsonrpc\""));
    assert!(logs.contains("sent invalid JSON-RPC"));
    assert!(logs.contains("active_worker_count=1"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "downstream",
        "invalid_jsonrpc",
        "downstream_protocol_violation",
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reports_downstream_binary_protocol_violations() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_that_sends_binary_after_initialize().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-downstream-binary".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-downstream-binary".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        let frame = wait_for_close_frame(&mut websocket).await;
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::ERROR
        );
        assert_eq!(
            close_frame
                .reason
                .starts_with("downstream app-server disconnected:"),
            true
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(logs.contains("gateway v2 connection completed"), "{logs}");
    assert!(
        logs.contains("outcome=\"downstream_protocol_violation\""),
        "{logs}"
    );
    assert!(logs.contains("tenant-downstream-binary"), "{logs}");
    assert!(logs.contains("project-downstream-binary"), "{logs}");
    assert!(logs.contains("reason=\"invalid_binary\""), "{logs}");
    assert!(logs.contains("sent non-text JSON-RPC frame"), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_protocol_violation_and_connection_metrics(
        &metrics,
        "downstream",
        "invalid_binary",
        "downstream_protocol_violation",
    );
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        Some("downstream_protocol_violation".to_string())
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reports_downstream_unexpected_response_id_as_protocol_violation() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_that_sends_unexpected_response_after_initialize().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-downstream-response".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-downstream-response"
                .parse()
                .expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert!(
            timeout(Duration::from_millis(200), websocket.next())
                .await
                .is_err(),
            "unexpected downstream response should not close the gateway connection immediately"
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(
        logs.contains("ignoring unexpected JSON-RPC response from remote app server"),
        "{logs}"
    );
    assert!(logs.contains("tenant-downstream-response"), "{logs}");
    assert!(logs.contains("project-downstream-response"), "{logs}");
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        None
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reports_downstream_unexpected_error_id_as_protocol_violation() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_that_sends_unexpected_error_after_initialize().await;
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
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
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-downstream-error".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-downstream-error".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert!(
            timeout(Duration::from_millis(200), websocket.next())
                .await
                .is_err(),
            "unexpected downstream error should not close the gateway connection immediately"
        );
    })
    .await;
    assert!(logs.contains("codex_gateway.audit"), "{logs}");
    assert!(
        logs.contains("ignoring unexpected JSON-RPC error from remote app server"),
        "{logs}"
    );
    assert!(logs.contains("tenant-downstream-error"), "{logs}");
    assert!(logs.contains("project-downstream-error"), "{logs}");
    assert_eq!(
        observability
            .v2_connection_health()
            .snapshot()
            .last_connection_outcome,
        None
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_truncates_close_reason_when_downstream_disconnect_reason_is_long() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_that_disconnects_after_initialize_with_reason("x".repeat(200))
            .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::ERROR
    );
    assert_eq!(
        close_frame
            .reason
            .starts_with("downstream app-server disconnected:"),
        true
    );
    assert_eq!(close_frame.reason.len() <= 123, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn invalid_payload_close_reason_is_truncated_to_protocol_limit() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(|websocket: WebSocketUpgrade| async move {
            websocket.on_upgrade(|mut socket| async move {
                let observability = GatewayObservability::default();
                let request_context = GatewayRequestContext::default();
                super::super::super::send_observed_invalid_payload_close(
                    &mut socket,
                    &observability,
                    &request_context,
                    &std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "{}: {}",
                            super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON,
                            "x".repeat(200)
                        ),
                    ),
                    Duration::from_secs(10),
                )
                .await
                .expect("invalid payload close should send");
            })
        }),
    );
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server should run");
    });

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::PROTOCOL
    );
    assert_eq!(close_frame.reason.len() <= 123, true);
    assert!(
        close_frame
            .reason
            .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn handle_app_server_event_closes_with_policy_reason_when_downstream_lags() {
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let metrics_for_server = metrics.clone();
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let websocket_url = websocket_url.clone();
            let metrics = metrics_for_server.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::new(Some(metrics), false);
                    let scope_registry = Arc::new(GatewayScopeRegistry::default());
                    let request_context = GatewayRequestContext {
                        tenant_id: "tenant-a".to_string(),
                        project_id: Some("project-a".to_string()),
                    };
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_secs(10),
                        max_pending_server_requests: 4,
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
                        test_initialize_response().await,
                    );
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::new(),
                        skills_changed_pending_refresh: false,
                        forwarded_connection_notifications: HashMap::new(),
                    };
                    let should_close = handle_app_server_event(
                        &mut socket,
                        &mut router,
                        &session_factory,
                        &connection,
                        &mut event_state,
                        &HashMap::new(),
                        DownstreamWorkerEvent {
                            worker_id: None,
                            event: Some(AppServerEvent::Lagged { skipped: 3 }),
                        },
                    )
                    .await
                    .expect("lagged event should be handled");
                    assert_eq!(
                        should_close.map(|close| close.reject_pending_server_requests),
                        Some(true)
                    );
                })
            }
        }),
    );
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server should run");
    });

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(
        u16::from(close_frame.code),
        axum::extract::ws::close_code::POLICY
    );
    assert_eq!(
        close_frame.reason,
        "downstream app-server event stream lagged: skipped 3 events"
    );

    server_task.abort();
    let _ = server_task.await;
    assert_v2_downstream_backpressure_metric(&metrics, "none");
}

#[tokio::test]
async fn handle_app_server_event_records_notification_send_failure_metric() {
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
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_millis(1),
                        max_pending_server_requests: 4,
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
                    let warning = ServerNotification::Warning(WarningNotification {
                        thread_id: None,
                        message: "w".repeat(1024 * 1024),
                    });

                    for _ in 0..256 {
                        let result = handle_app_server_event(
                            &mut socket,
                            &mut router,
                            &session_factory,
                            &connection,
                            &mut event_state,
                            &HashMap::new(),
                            DownstreamWorkerEvent {
                                worker_id: None,
                                event: Some(AppServerEvent::ServerNotification(warning.clone())),
                            },
                        )
                        .await;
                        if let Err(err) = result {
                            let _ = outcome_tx
                                .send(
                                    super::super::super::classify_v2_connection_error(&err)
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
        .expect("notification send should fail")
        .expect("notification send outcome should be recorded");

    assert_eq!(outcome, "client_send_timed_out");
    assert_v2_notification_send_failure_metric(&metrics, "warning", "client_send_timed_out");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn send_client_jsonrpc_records_client_response_send_failure_metric() {
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
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_millis(1),
                        max_pending_server_requests: 4,
                        max_pending_client_requests: 4,
                        opt_out_notification_methods: HashSet::new(),
                    };
                    let request_id = RequestId::String("model-list-1".to_string());
                    let response = JSONRPCMessage::Response(JSONRPCResponse {
                        id: request_id.clone(),
                        result: serde_json::json!({
                            "models": ["m".repeat(1024 * 1024)],
                        }),
                    });

                    for _ in 0..256 {
                        let result = super::super::super::send_client_jsonrpc(
                            &mut socket,
                            &connection,
                            &request_id,
                            "model/list",
                            response.clone(),
                        )
                        .await;
                        if let Err(err) = result {
                            let _ = outcome_tx
                                .send(
                                    super::super::super::classify_v2_connection_error(&err)
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
        .expect("client response send should fail")
        .expect("client response send outcome should be recorded");

    assert_eq!(outcome, "client_send_timed_out");
    assert_v2_client_response_send_failure_metric(&metrics, "model/list", "client_send_timed_out");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn send_client_jsonrpc_error_records_client_response_send_failure_metric() {
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
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_millis(1),
                        max_pending_server_requests: 4,
                        max_pending_client_requests: 4,
                        opt_out_notification_methods: HashSet::new(),
                    };
                    let request_id = RequestId::String("thread-read-1".to_string());

                    for _ in 0..256 {
                        let result = super::super::super::send_client_jsonrpc_error(
                            &mut socket,
                            &connection,
                            &request_id,
                            "thread/read",
                            JSONRPCErrorError {
                                code: super::super::super::INTERNAL_ERROR_CODE,
                                message: "m".repeat(1024 * 1024),
                                data: None,
                            },
                        )
                        .await;
                        if let Err(err) = result {
                            let _ = outcome_tx
                                .send(
                                    super::super::super::classify_v2_connection_error(&err)
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
        .expect("client error response send should fail")
        .expect("client error response send outcome should be recorded");

    assert_eq!(outcome, "client_send_timed_out");
    assert_v2_client_response_send_failure_metric(&metrics, "thread/read", "client_send_timed_out");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn send_observed_close_frame_records_send_failure_metric() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async({
        let metrics = metrics.clone();
        async move {
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
                            let observability = GatewayObservability::new(Some(metrics), false);
                            let request_context = GatewayRequestContext::default();

                            for _ in 0..256 {
                                let result = super::super::super::send_observed_close_frame(
                                    &mut socket,
                                    &observability,
                                    &request_context,
                                    close_code::POLICY,
                                    "gateway initialize timed out",
                                    Duration::from_millis(1),
                                )
                                .await;
                                if let Err(err) = result {
                                    let _ = outcome_tx
                                        .send(
                                            super::super::super::classify_v2_connection_error(&err)
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
                .expect("close frame send should fail")
                .expect("close frame send outcome should be recorded");

            assert_eq!(outcome, "connection_error");

            server_task.abort();
            let _ = server_task.await;
        }
    })
    .await;

    assert_v2_close_frame_send_failure_metric(&metrics, close_code::POLICY, "connection_error");
    assert!(logs.contains("failed to deliver gateway v2 close frame to northbound client"));
    assert!(logs.contains("tenant_id=\"default\""));
    assert!(logs.contains("code=1008"));
    assert!(logs.contains("reason=\"gateway initialize timed out\""));
    assert!(logs.contains("outcome=\"connection_error\""));
}

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
                                    super::super::super::classify_v2_connection_error(&err)
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
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_downstream_lags() {
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let websocket_url = websocket_url.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
                    let scope_registry = Arc::new(GatewayScopeRegistry::default());
                    let request_context = GatewayRequestContext {
                        tenant_id: "tenant-visible".to_string(),
                        project_id: Some("project-visible".to_string()),
                    };
                    let connection = GatewayV2ConnectionContext {
                        admission: &admission,
                        observability: &observability,
                        scope_registry: &scope_registry,
                        request_context: &request_context,
                        client_send_timeout: Duration::from_secs(10),
                        max_pending_server_requests: 4,
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
                        test_initialize_response().await,
                    );
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(2),
                                request_id: RequestId::String("downstream-resolved-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String(
                                    "gateway-resolved-1".to_string(),
                                ),
                                worker_websocket_url: test_worker_websocket_url(Some(2)),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: Some("thread-visible".to_string()),
                            },
                        )]),
                        skills_changed_pending_refresh: false,
                        forwarded_connection_notifications: HashMap::new(),
                    };
                    let should_close = handle_app_server_event(
                        &mut socket,
                        &mut router,
                        &session_factory,
                        &connection,
                        &mut event_state,
                        &HashMap::new(),
                        DownstreamWorkerEvent {
                            worker_id: Some(2),
                            event: Some(AppServerEvent::Lagged { skipped: 3 }),
                        },
                    )
                    .await
                    .expect("lagged event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| { (close.outcome, close.reject_pending_server_requests) }),
                        Some(("downstream_backpressure", true))
                    );
                })
            }
        }),
    );
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server should run");
    });

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let frame = wait_for_close_frame(&mut websocket).await;
        let Message::Close(Some(close_frame)) = frame else {
            panic!("expected websocket close frame");
        };
        assert_eq!(
            u16::from(close_frame.code),
            axum::extract::ws::close_code::POLICY
        );
        assert_eq!(
            close_frame.reason,
            "downstream app-server event stream lagged: skipped 3 events"
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
        "closing gateway v2 connection because the downstream app-server event stream lagged"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(2)"));
    assert!(logs.contains("skipped_event_count=3"));
    assert!(logs.contains("pending_server_request_count=0"));
    assert!(logs.contains("pending_server_request_ids=[]"));
    assert!(logs.contains("answered_but_unresolved_server_request_count=1"));
    assert!(
        logs.contains(
            "answered_but_unresolved_gateway_request_ids=[String(\"gateway-resolved-1\")]"
        )
    );
    assert!(logs.contains(
        "answered_but_unresolved_downstream_request_ids=[String(\"downstream-resolved-1\")]"
    ));
    assert!(logs.contains("answered_but_unresolved_worker_ids=[2]"));
}

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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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
            .expect("dynamic tool request should send");

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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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
                    id: RequestId::String("server-request-1".to_string()),
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
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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
            .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
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
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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
                .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
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
                .starts_with(super::super::super::INVALID_CLIENT_JSONRPC_PAYLOAD_CLOSE_REASON)
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
                    assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
                    assert_eq!(
                        error.error.message,
                        super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
                    );
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
                max_pending_server_requests:
                    super::super::super::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
                max_pending_client_requests:
                    super::super::super::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
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
                max_pending_server_requests:
                    super::super::super::MAX_PENDING_SERVER_REQUESTS_PER_CONNECTION,
                max_pending_client_requests:
                    super::super::super::MAX_PENDING_CLIENT_REQUESTS_PER_CONNECTION,
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

#[tokio::test]
async fn websocket_upgrade_closes_when_client_responds_to_unknown_server_request() {
    let initialize_response = test_initialize_response().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: start_mock_remote_server_for_passthrough_request_with_result(
                        "account/read",
                        serde_json::json!({}),
                        serde_json::json!({
                            "account": null,
                            "reasoningModelConfig": null,
                        }),
                    )
                    .await,
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
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("unknown-server-request".to_string()),
                result: serde_json::json!({}),
            }))
            .expect("unexpected response should serialize")
            .into(),
        ))
        .await
        .expect("unexpected response should send");

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::PROTOCOL);
    assert_eq!(
        close_frame.reason,
        "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_server_request_lifecycle_and_connection_metrics(
        &metrics,
        "unexpected_client_server_request_response",
        "response",
        "protocol_violation",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_rejects_pending_server_requests_when_client_responds_to_unknown_server_request()
 {
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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("unknown-server-request".to_string()),
                result: serde_json::json!({}),
            }))
            .expect("unexpected response should serialize")
            .into(),
        ))
        .await
        .expect("unexpected response should send");

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::PROTOCOL);
    assert_eq!(
        close_frame.reason,
        "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_server_request_lifecycle_and_connection_metrics(
        &metrics,
        "unexpected_client_server_request_response",
        "response",
        "protocol_violation",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_scope_and_pending_request_ids_when_client_responds_to_unknown_server_request()
 {
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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: RequestId::String("unknown-server-request".to_string()),
                        result: serde_json::json!({}),
                    }))
                    .expect("unexpected response should serialize")
                    .into(),
                ))
                .await
                .expect("unexpected response should send");

            let frame = wait_for_close_frame(&mut websocket).await;
            let Message::Close(Some(close_frame)) = frame else {
                panic!("expected websocket close frame");
            };
            assert_eq!(
                close_frame.reason,
                "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
            );

            server_task.abort();
            let _ = server_task.await;
        })
        .await;

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("response"));
    assert!(logs.contains("unexpected_request_id=String(\"unknown-server-request\")"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains("pending_downstream_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains("pending_worker_ids=[0]"));
    assert!(logs.contains("pending_worker_websocket_urls=["));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=0"));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("protocol_violation"));
}

#[tokio::test]
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_client_responds_to_unknown_server_request()
 {
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
                        result: serde_json::json!({ "approved": true }),
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
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: RequestId::String("unknown-server-request".to_string()),
                        result: serde_json::json!({}),
                    }))
                    .expect("unexpected response should serialize")
                    .into(),
                ))
                .await
                .expect("unexpected response should send");

            let frame = wait_for_close_frame(&mut websocket).await;
            let Message::Close(Some(close_frame)) = frame else {
                panic!("expected websocket close frame");
            };
            assert_eq!(
                close_frame.reason,
                "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
            );

            release_downstream_tx
                .send(())
                .expect("downstream release should send");
            server_task.abort();
            let _ = server_task.await;
        })
        .await;

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("unexpected_request_id=String(\"unknown-server-request\")"));
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
    assert!(logs.contains("protocol_violation"));
}

#[tokio::test]
async fn websocket_upgrade_rejects_pending_server_requests_when_client_errors_unknown_server_request()
 {
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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                id: RequestId::String("unknown-server-request".to_string()),
                error: JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                },
            }))
            .expect("unexpected error should serialize")
            .into(),
        ))
        .await
        .expect("unexpected error should send");

    let frame = wait_for_close_frame(&mut websocket).await;
    let Message::Close(Some(close_frame)) = frame else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::PROTOCOL);
    assert_eq!(
        close_frame.reason,
        "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_v2_server_request_lifecycle_and_connection_metrics(
        &metrics,
        "unexpected_client_server_request_response",
        "error",
        "protocol_violation",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_scope_and_pending_request_ids_when_client_errors_unknown_server_request()
 {
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
        assert_eq!(error.error.code, super::super::super::INTERNAL_ERROR_CODE);
        assert_eq!(
            error.error.message,
            super::super::super::PENDING_SERVER_REQUEST_ABORTED_MESSAGE
        );
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
                    serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                        id: RequestId::String("unknown-server-request".to_string()),
                        error: JSONRPCErrorError {
                            code: -32000,
                            message: "user rejected".to_string(),
                            data: None,
                        },
                    }))
                    .expect("unexpected error should serialize")
                    .into(),
                ))
                .await
                .expect("unexpected error should send");

            let frame = wait_for_close_frame(&mut websocket).await;
            let Message::Close(Some(close_frame)) = frame else {
                panic!("expected websocket close frame");
            };
            assert_eq!(
                close_frame.reason,
                "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
            );

            server_task.abort();
            let _ = server_task.await;
        })
        .await;

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("error"));
    assert!(logs.contains("unexpected_request_id=String(\"unknown-server-request\")"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"server-request-1\")]"));
    assert!(logs.contains("thread_scoped_pending_server_request_count=1"));
    assert!(logs.contains("connection_scoped_pending_server_request_count=0"));
    assert!(logs.contains("connection_outcome"));
    assert!(logs.contains("protocol_violation"));
}

#[tokio::test]
async fn websocket_upgrade_logs_answered_but_unresolved_routes_when_client_errors_unknown_server_request()
 {
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
                        result: serde_json::json!({ "approved": true }),
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
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                        id: RequestId::String("unknown-server-request".to_string()),
                        error: JSONRPCErrorError {
                            code: -32000,
                            message: "user rejected".to_string(),
                            data: None,
                        },
                    }))
                    .expect("unexpected error should serialize")
                    .into(),
                ))
                .await
                .expect("unexpected error should send");

            let frame = wait_for_close_frame(&mut websocket).await;
            let Message::Close(Some(close_frame)) = frame else {
                panic!("expected websocket close frame");
            };
            assert_eq!(
                close_frame.reason,
                "unexpected gateway websocket server-request response: String(\"unknown-server-request\")"
            );

            release_downstream_tx
                .send(())
                .expect("downstream release should send");
            server_task.abort();
            let _ = server_task.await;
        })
        .await;

    assert!(
        logs.contains("gateway v2 client replied to a server request that is no longer pending")
    );
    assert!(logs.contains(
        "rejecting unresolved downstream server requests because the gateway v2 connection ended"
    ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("response_kind"));
    assert!(logs.contains("error"));
    assert!(logs.contains("unexpected_request_id=String(\"unknown-server-request\")"));
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
    assert!(logs.contains("protocol_violation"));
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

#[tokio::test]
async fn websocket_upgrade_forwards_account_read_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "account/read",
        serde_json::json!({
            "refreshToken": false,
        }),
        serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "gateway@example.com",
                "planType": "plus",
            },
            "requiresOpenaiAuth": false,
        }),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("account-read".to_string()),
                method: "account/read".to_string(),
                params: Some(serde_json::json!({
                    "refreshToken": false,
                })),
                trace: None,
            }))
            .expect("account read request should serialize")
            .into(),
        ))
        .await
        .expect("account read request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected account read response");
    };
    assert_eq!(response.id, RequestId::String("account-read".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "gateway@example.com",
                "planType": "plus",
            },
            "requiresOpenaiAuth": false,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_account_rate_limits_read_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "account/rateLimits/read",
            None,
            serde_json::json!({
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 42,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": "plus",
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": 42,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_000,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": "plus",
                        "rateLimitReachedType": null,
                    }
                },
            }),
        )
        .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("account-rate-limits-read".to_string()),
                method: "account/rateLimits/read".to_string(),
                params: None,
                trace: None,
            }))
            .expect("account rate limits read request should serialize")
            .into(),
        ))
        .await
        .expect("account rate limits read request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected account rate limits read response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-rate-limits-read".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "rateLimits": {
                "limitId": "codex",
                "limitName": "Codex",
                "primary": {
                    "usedPercent": 42,
                    "windowMinutes": 300,
                    "resetsAt": 1_700_000_000,
                },
                "secondary": null,
                "credits": null,
                "planType": "plus",
                "rateLimitReachedType": null,
            },
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 42,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": "plus",
                    "rateLimitReachedType": null,
                }
            },
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_bootstrap_setup_discovery_requests() {
    let cases = vec![
        (
            "externalAgentConfig/detect",
            "external-agent-config-detect",
            Some(serde_json::json!({
                "includeHome": true,
                "cwds": ["/tmp/project"],
            })),
            serde_json::json!({
                "items": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import CLAUDE.md from /tmp/project",
                    "cwd": "/tmp/project",
                    "details": null,
                }],
            }),
        ),
        (
            "app/list",
            "app-list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 25,
                "threadId": null,
            })),
            serde_json::json!({
                "data": [{
                    "id": "calendar",
                    "name": "Calendar",
                    "description": "Calendar connector",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
        ),
        (
            "skills/list",
            "skills-list",
            Some(serde_json::json!({
                "cwds": ["/tmp/project"],
                "forceReload": true,
                "perCwdExtraUserRoots": null,
            })),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/project",
                    "skills": [{
                        "name": "gateway-skill",
                        "description": "Gateway passthrough skill",
                        "shortDescription": "Gateway skill",
                        "interface": null,
                        "dependencies": null,
                        "path": "/tmp/project/.codex/skills/gateway-skill/SKILL.md",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
        ),
        (
            "plugin/list",
            "plugin-list",
            Some(serde_json::json!({
                "cwds": ["/tmp/project"],
            })),
            serde_json::json!({
                "marketplaces": [{
                    "name": "demo-marketplace",
                    "path": "/tmp/project/plugins/demo-marketplace.json",
                    "interface": {
                        "displayName": "Demo Marketplace",
                    },
                    "plugins": [{
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [],
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params,
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_plugin_and_setup_mutation_requests() {
    let cases = vec![
        (
            "externalAgentConfig/import",
            "external-agent-config-import",
            Some(serde_json::json!({
                "migrationItems": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import CLAUDE.md from /tmp/project",
                    "cwd": "/tmp/project",
                    "details": null,
                }],
            })),
            serde_json::json!({}),
        ),
        (
            "plugin/read",
            "plugin-read",
            Some(serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            })),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            "plugin-install",
            Some(serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            })),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            "plugin-uninstall",
            Some(serde_json::json!({
                "pluginId": "demo-plugin@local",
            })),
            serde_json::json!({}),
        ),
        (
            "config/batchWrite",
            "config-batch-write",
            Some(serde_json::json!({
                "edits": [],
                "filePath": null,
                "expectedVersion": null,
                "reloadUserConfig": true,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "remote-version-2",
                "filePath": "/tmp/project/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        ("memory/reset", "memory-reset", None, serde_json::json!({})),
        (
            "account/logout",
            "account-logout",
            None,
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params,
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_core_thread_workflow_requests() {
    let cases = vec![
        (
            "thread/start",
            "thread-start",
            None,
            None,
            Some(serde_json::json!({
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "baseInstructions": null,
                "config": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "developerInstructions": null,
                "dynamicTools": null,
                "ephemeral": true,
                "experimentalRawEvents": false,
                "mockExperimentalField": null,
                "persistExtendedHistory": false,
                "personality": null,
                "sandbox": null,
                "serviceName": null,
                "sessionStartSource": null,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-started",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/resume",
            "thread-resume",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "history": null,
                "path": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "sandbox": null,
                "config": null,
                "baseInstructions": null,
                "developerInstructions": null,
                "personality": null,
                "persistExtendedHistory": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-resumed",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/fork",
            "thread-fork",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "path": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "sandbox": null,
                "config": null,
                "baseInstructions": null,
                "developerInstructions": null,
                "ephemeral": true,
                "persistExtendedHistory": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-forked",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/list",
            "thread-list",
            None,
            Some("thread-visible"),
            Some(serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": 10,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            })),
            serde_json::json!({
                "data": [{
                    "id": "thread-visible",
                    "name": "Visible thread",
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            "thread-loaded-list",
            None,
            Some("thread-visible"),
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": ["thread-visible"],
                "nextCursor": null,
            }),
        ),
        (
            "thread/read",
            "thread-read",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "includeTurns": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
                    "name": "Visible thread",
                },
            }),
        ),
        (
            "thread/name/set",
            "thread-name-set",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "name": "Renamed thread",
            })),
            serde_json::json!({}),
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "mode": "enabled",
            })),
            serde_json::json!({}),
        ),
        (
            "turn/start",
            "turn-start",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "collaborationMode": null,
                "input": [],
                "cwd": "/tmp/project",
                "effort": null,
                "model": "gpt-5",
                "outputSchema": null,
                "personality": null,
                "responsesapiClientMetadata": null,
                "sandboxPolicy": null,
                "summary": null,
                "threadId": "thread-visible",
            })),
            serde_json::json!({
                "turn": {
                    "id": "turn-1",
                },
            }),
        ),
    ];

    for (method, request_id, thread_id, pre_registered_thread_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = thread_id {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        if let Some(thread_id) = pre_registered_thread_id {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params,
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_account_login_and_feedback_requests() {
    let cases = vec![
        (
            "account/login/start",
            "account-login-start",
            serde_json::json!({
                "type": "chatgpt",
            }),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-1",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            "account-login-cancel",
            serde_json::json!({
                "loginId": "login-1",
            }),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "feedback/upload",
            "feedback-upload",
            serde_json::json!({
                "classification": "bug",
                "reason": "gateway feedback request",
                "threadId": "thread-visible",
                "includeLogs": true,
                "extraLogFiles": ["/tmp/rollout.jsonl"],
                "tags": {
                    "turn_id": "turn-1",
                },
            }),
            serde_json::json!({
                "threadId": "feedback-thread-1",
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = params.get("threadId").and_then(Value::as_str) {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
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
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("passthrough request should serialize")
                .into(),
            ))
            .await
            .expect("passthrough request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected passthrough response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_fuzzy_file_search_requests() {
    let cases = vec![
        (
            "fuzzyFileSearch",
            "fuzzy-file-search",
            serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-1",
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/project",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 42,
                    "indices": [5, 6, 7, 8],
                }],
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            "fuzzy-file-search-session-start",
            serde_json::json!({
                "sessionId": "search-session-1",
                "roots": ["/tmp/project"],
            }),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            "fuzzy-file-search-session-update",
            serde_json::json!({
                "sessionId": "search-session-1",
                "query": "gate",
            }),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            "fuzzy-file-search-session-stop",
            serde_json::json!({
                "sessionId": "search-session-1",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
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
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("fuzzy file search request should serialize")
                .into(),
            ))
            .await
            .expect("fuzzy file search request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected fuzzy file search response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_filesystem_operation_requests() {
    let cases = vec![
        (
            "fs/readFile",
            "fs-read-file",
            serde_json::json!({
                "path": "/tmp/project/input.txt",
            }),
            serde_json::json!({
                "dataBase64": "Z2F0ZXdheS1maWxl",
            }),
        ),
        (
            "fs/writeFile",
            "fs-write-file",
            serde_json::json!({
                "path": "/tmp/project/output.txt",
                "dataBase64": "Z2F0ZXdheS13cml0ZQ==",
            }),
            serde_json::json!({}),
        ),
        (
            "fs/createDirectory",
            "fs-create-directory",
            serde_json::json!({
                "path": "/tmp/project/nested",
                "recursive": true,
            }),
            serde_json::json!({}),
        ),
        (
            "fs/getMetadata",
            "fs-get-metadata",
            serde_json::json!({
                "path": "/tmp/project/output.txt",
            }),
            serde_json::json!({
                "isDirectory": false,
                "isFile": true,
                "isSymlink": false,
                "createdAtMs": 0,
                "modifiedAtMs": 0,
            }),
        ),
        (
            "fs/readDirectory",
            "fs-read-directory",
            serde_json::json!({
                "path": "/tmp/project",
            }),
            serde_json::json!({
                "entries": [{
                    "fileName": "output.txt",
                    "isDirectory": false,
                    "isFile": true,
                }],
            }),
        ),
        (
            "fs/copy",
            "fs-copy",
            serde_json::json!({
                "sourcePath": "/tmp/project/output.txt",
                "destinationPath": "/tmp/project/copy.txt",
            }),
            serde_json::json!({}),
        ),
        (
            "fs/remove",
            "fs-remove",
            serde_json::json!({
                "path": "/tmp/project/copy.txt",
                "recursive": true,
                "force": true,
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
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
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("filesystem request should serialize")
                .into(),
            ))
            .await
            .expect("filesystem request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected filesystem response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_config_value_write_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "config/value/write",
        serde_json::json!({
            "keyPath": "plugins.demo-plugin",
            "value": {
                "enabled": true,
            },
            "mergeStrategy": "upsert",
            "filePath": null,
            "expectedVersion": null,
        }),
        serde_json::json!({
            "status": "ok",
            "version": "remote-version-1",
            "filePath": "/tmp/remote-project/config.toml",
            "overriddenMetadata": null,
        }),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("config-value-write".to_string()),
                method: "config/value/write".to_string(),
                params: Some(serde_json::json!({
                    "keyPath": "plugins.demo-plugin",
                    "value": {
                        "enabled": true,
                    },
                    "mergeStrategy": "upsert",
                    "filePath": null,
                    "expectedVersion": null,
                })),
                trace: None,
            }))
            .expect("config value write request should serialize")
            .into(),
        ))
        .await
        .expect("config value write request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected config value write response");
    };
    assert_eq!(
        response.id,
        RequestId::String("config-value-write".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "status": "ok",
            "version": "remote-version-1",
            "filePath": "/tmp/remote-project/config.toml",
            "overriddenMetadata": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_model_list_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "model/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
            "includeHidden": true,
        }),
        serde_json::json!({
            "data": [
                {
                    "model": "gpt-5",
                    "provider": "openai",
                    "contextWindow": 272000,
                    "maxOutputTokens": 32000,
                    "supportsImages": true,
                    "supportsPromptCacheKey": true,
                    "supportsResponseSchema": true,
                    "supportsReasoningSummaries": true,
                    "supportsEncryptedReasoningContent": false,
                    "supportsReasoningEffort": true,
                    "supportsCustomToolCallInput": true,
                    "supportsParallelToolCalls": true,
                    "supportsToolChoiceRequired": true,
                    "supportsTerminalToolCall": true,
                    "supportsPreserveBackground": true,
                    "supportsMinimalEffortReasoning": true,
                    "supportsVerbosity": true,
                    "overrideRank": null,
                    "upgradeInfo": null,
                    "availabilityNux": null,
                    "displayName": "GPT-5",
                    "description": "Gateway test model",
                    "hidden": false,
                    "supportedReasoningEfforts": [
                        {
                            "reasoningEffort": "medium",
                            "description": "Balanced",
                        }
                    ],
                    "defaultReasoningEffort": "medium",
                    "inputModalities": ["text"],
                    "supportsPersonality": false,
                    "additionalSpeedTiers": [],
                    "isDefault": true,
                }
            ],
            "nextCursor": null,
        }),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("model-list".to_string()),
                method: "model/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": null,
                    "includeHidden": true,
                })),
                trace: None,
            }))
            .expect("model list request should serialize")
            .into(),
        ))
        .await
        .expect("model list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected model list response");
    };
    assert_eq!(response.id, RequestId::String("model-list".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": [
                {
                    "model": "gpt-5",
                    "provider": "openai",
                    "contextWindow": 272000,
                    "maxOutputTokens": 32000,
                    "supportsImages": true,
                    "supportsPromptCacheKey": true,
                    "supportsResponseSchema": true,
                    "supportsReasoningSummaries": true,
                    "supportsEncryptedReasoningContent": false,
                    "supportsReasoningEffort": true,
                    "supportsCustomToolCallInput": true,
                    "supportsParallelToolCalls": true,
                    "supportsToolChoiceRequired": true,
                    "supportsTerminalToolCall": true,
                    "supportsPreserveBackground": true,
                    "supportsMinimalEffortReasoning": true,
                    "supportsVerbosity": true,
                    "overrideRank": null,
                    "upgradeInfo": null,
                    "availabilityNux": null,
                    "displayName": "GPT-5",
                    "description": "Gateway test model",
                    "hidden": false,
                    "supportedReasoningEfforts": [
                        {
                            "reasoningEffort": "medium",
                            "description": "Balanced",
                        }
                    ],
                    "defaultReasoningEffort": "medium",
                    "inputModalities": ["text"],
                    "supportsPersonality": false,
                    "additionalSpeedTiers": [],
                    "isDefault": true,
                }
            ],
            "nextCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_server_status_list_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "mcpServerStatus/list",
        serde_json::json!({
            "cursor": null,
            "limit": 100,
            "detail": "toolsAndAuthOnly",
        }),
        serde_json::json!({
            "data": [{
                "name": "calendar",
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken"
            }],
            "nextCursor": null,
        }),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("mcp-status-list".to_string()),
                method: "mcpServerStatus/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 100,
                    "detail": "toolsAndAuthOnly",
                })),
                trace: None,
            }))
            .expect("mcp status list request should serialize")
            .into(),
        ))
        .await
        .expect("mcp status list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected mcp status list response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-status-list".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": [{
                "name": "calendar",
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken"
            }],
            "nextCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_server_oauth_login_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "mcpServer/oauth/login",
        serde_json::json!({
            "name": "calendar",
            "scopes": ["calendar.read"],
            "timeoutSecs": 120,
        }),
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/calendar",
        }),
    )
    .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
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
                id: RequestId::String("mcp-oauth-login".to_string()),
                method: "mcpServer/oauth/login".to_string(),
                params: Some(serde_json::json!({
                    "name": "calendar",
                    "scopes": ["calendar.read"],
                    "timeoutSecs": 120,
                })),
                trace: None,
            }))
            .expect("mcp oauth login request should serialize")
            .into(),
        ))
        .await
        .expect("mcp oauth login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected mcp oauth login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-oauth-login".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/calendar",
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_additional_thread_control_requests() {
    let cases = vec![
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/shellCommand",
            "thread-shell-command",
            serde_json::json!({
                "threadId": "thread-visible",
                "command": "git status --short",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/backgroundTerminals/clean",
            "thread-background-terminals-clean",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/rollback",
            "thread-rollback",
            serde_json::json!({
                "threadId": "thread-visible",
                "numTurns": 2,
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
                    "name": "Visible thread",
                    "turns": [],
                },
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

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
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}
