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

#[path = "v2_tests_cases_1_forwarding_account_setup.rs"]
mod v2_tests_cases_1_forwarding_account_setup;

#[path = "v2_tests_cases_1_forwarding_account_setup_read.rs"]
mod v2_tests_cases_1_forwarding_account_setup_read;

#[path = "v2_tests_cases_1_forwarding_account_login.rs"]
mod v2_tests_cases_1_forwarding_account_login;

#[path = "v2_tests_cases_1_forwarding_server_requests.rs"]
mod v2_tests_cases_1_forwarding_server_requests;

#[path = "v2_tests_cases_1_downstream_protocol.rs"]
mod v2_tests_cases_1_downstream_protocol;

#[path = "v2_tests_cases_1_websocket_lifecycle.rs"]
mod v2_tests_cases_1_websocket_lifecycle;

#[path = "v2_tests_cases_1_forwarding_requests.rs"]
mod v2_tests_cases_1_forwarding_requests;

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
