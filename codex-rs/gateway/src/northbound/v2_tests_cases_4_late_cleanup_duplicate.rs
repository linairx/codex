use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2::DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON;

#[tokio::test]
async fn websocket_upgrade_closes_when_downstream_reuses_pending_server_request_id() {
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
    let test_metrics = metrics.clone();
    let app = Router::new().route(
            "/",
            any(move |websocket: WebSocketUpgrade| {
                let websocket_url = websocket_url.clone();
                let metrics = test_metrics.clone();
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
                            pending_server_requests: HashMap::from([(
                                RequestId::String("downstream-request-1".to_string()),
                                PendingServerRequestRoute {
                                    worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                                    downstream_request_id: RequestId::String(
                                        "downstream-request-1".to_string(),
                                    ),                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                                },
                            )]),
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
                                worker_id: Some(0),
                                event: Some(AppServerEvent::ServerRequest(
                                    ServerRequest::ToolRequestUserInput {
                                        request_id: RequestId::String(
                                            "downstream-request-1".to_string(),
                                        ),
                                        params: codex_app_server_protocol::ToolRequestUserInputParams {
                                            auto_resolution_ms: None,
                                            thread_id: "thread-visible".to_string(),
                                            turn_id: "turn-visible".to_string(),
                                            item_id: "tool-call-2".to_string(),
                                            questions: vec![codex_app_server_protocol::ToolRequestUserInputQuestion {
                                                id: "mode".to_string(),
                                                header: "Mode".to_string(),
                                                question: "Continue?".to_string(),
                                                is_other: false,
                                                is_secret: false,
                                                options: Some(vec![codex_app_server_protocol::ToolRequestUserInputOption {
                                                    label: "yes".to_string(),
                                                    description: "Continue".to_string(),
                                                }]),
                                            }],
                                        },
                                    },
                                )),
                            },
                        )
                        .await
                        .expect("duplicate server request should be handled");
                        assert_eq!(
                            should_close
                                .map(|close| (close.outcome, close.reject_pending_server_requests)),
                            Some(("duplicate_downstream_server_request", true))
                        );
                    })
                }
            }),
        );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::ERROR);
    assert_eq!(
        close_frame.reason,
        DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[("duplicate_pending_request", "item/tool/requestUserInput", 1)],
    );
}

#[tokio::test]
async fn websocket_upgrade_logs_scope_worker_and_pending_ids_when_downstream_reuses_pending_server_request_id()
 {
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
                            pending_server_requests: HashMap::from([(
                                RequestId::String("downstream-request-1".to_string()),
                                PendingServerRequestRoute {
                                    worker_id: Some(0),
                        worker_websocket_url: test_worker_websocket_url(Some(0)),
                                    downstream_request_id: RequestId::String(
                                        "downstream-request-1".to_string(),
                                    ),                        method: "item/tool/requestUserInput".to_string(),
                        thread_id: Some("thread-visible".to_string()),
                                },
                            )]),
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
                                worker_id: Some(0),
                                event: Some(AppServerEvent::ServerRequest(
                                    ServerRequest::ToolRequestUserInput {
                                        request_id: RequestId::String(
                                            "downstream-request-1".to_string(),
                                        ),
                                        params: codex_app_server_protocol::ToolRequestUserInputParams {
                                            auto_resolution_ms: None,
                                            thread_id: "thread-visible".to_string(),
                                            turn_id: "turn-visible".to_string(),
                                            item_id: "tool-call-2".to_string(),
                                            questions: vec![codex_app_server_protocol::ToolRequestUserInputQuestion {
                                                id: "mode".to_string(),
                                                header: "Mode".to_string(),
                                                question: "Continue?".to_string(),
                                                is_other: false,
                                                is_secret: false,
                                                options: Some(vec![codex_app_server_protocol::ToolRequestUserInputOption {
                                                    label: "yes".to_string(),
                                                    description: "Continue".to_string(),
                                                }]),
                                            }],
                                        },
                                    },
                                )),
                            },
                        )
                        .await
                        .expect("duplicate server request should be handled");
                        assert_eq!(
                            should_close
                                .map(|close| (close.outcome, close.reject_pending_server_requests)),
                            Some(("duplicate_downstream_server_request", true))
                        );
                    })
                }
            }),
        );
    let logs = capture_logs_async(async move {
        let server = axum::serve(listener, app);
        let server_task = tokio::spawn(server.into_future());
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::ERROR);
        assert_eq!(
            close_frame.reason,
            DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains(
            "closing gateway v2 connection because a downstream session reused a pending server-request id"
        ));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("worker_id=Some(0)"));
    assert!(logs.contains("request_id=String(\"downstream-request-1\")"));
    assert!(logs.contains("item/tool/requestUserInput"));
    assert!(logs.contains("pending_server_request_count=1"));
    assert!(logs.contains("pending_server_request_ids=[String(\"downstream-request-1\")]"));
    assert!(
        logs.contains("pending_downstream_server_request_ids=[String(\"downstream-request-1\")]")
    );
    assert!(logs.contains("pending_thread_ids=[\"thread-visible\"]"));
    assert!(logs.contains("pending_worker_ids=[0]"));
    assert!(logs.contains("pending_worker_websocket_urls=["));
}
