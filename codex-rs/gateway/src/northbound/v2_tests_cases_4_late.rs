use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2::DOWNSTREAM_SESSION_ENDED_CLOSE_REASON;
use crate::northbound::v2::DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2::STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2_connection::GatewayV2ReconnectState;

#[tokio::test]
async fn reconnectable_router_keeps_multi_worker_topology_with_one_live_worker() {
    let (event_tx, event_rx) = mpsc::channel(1);
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
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
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://worker-b.invalid".to_string(),
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
    );
    let reconnect_state = GatewayV2ReconnectState {
        configured_worker_ids: vec![0, 1],
        worker_websocket_urls: vec![
            "ws://worker-a.invalid".to_string(),
            "ws://worker-b.invalid".to_string(),
        ],
        session_factory,
        initialize_params: InitializeParams {
            client_info: ClientInfo {
                name: "codex-tui".to_string(),
                title: None,
                version: "0.0.0-test".to_string(),
            },
            capabilities: None,
        },
        request_context: GatewayRequestContext::default(),
        retry_backoff: Duration::from_secs(1),
    };
    let router = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(reconnect_state),
    };

    assert!(router.multi_worker_topology());
}

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

#[tokio::test]
async fn websocket_upgrade_closes_when_worker_disconnects_with_pending_connection_server_request() {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: Some(0),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
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
        STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_worker_cleanup_ids_when_worker_disconnects_with_pending_connection_server_request()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let expected_worker_a_url = worker_a.clone();
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            let operator_events_tx = operator_events_tx.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability =
                        GatewayObservability::default().with_operator_events(operator_events_tx);
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: Some(0),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
                    );
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::ERROR);
        assert_eq!(
            close_frame.reason,
            STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains("downstream worker disconnected within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(0)"));
    assert!(logs.contains("remaining_worker_count=1"));
    assert!(logs.contains("disconnect_message"));
    assert!(logs.contains("worker-a lost"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=0"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=1"));
    assert!(
        logs.contains("stranded_connection_scoped_server_request_ids=[String(\"gateway-srv-1\")]")
    );
    let cleanup_event = operator_events_rx
        .try_recv()
        .expect("worker cleanup event should publish");
    assert_eq!(cleanup_event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(
        cleanup_event.data,
        serde_json::json!({
            "workerId": 0,
            "workerWebsocketUrl": expected_worker_a_url,
            "remainingWorkerCount": 1,
            "disconnectMessage": "worker-a lost",
            "resolvedThreadScopedServerRequestCount": 0,
            "resolvedThreadScopedServerRequestIds": [],
            "resolvedThreadScopedDownstreamServerRequestIds": [],
            "resolvedThreadScopedServerRequestMethods": [],
            "resolvedThreadScopedThreadIds": [],
            "strandedConnectionScopedServerRequestCount": 1,
            "strandedConnectionScopedServerRequestIds": ["gateway-srv-1"],
            "strandedConnectionScopedDownstreamServerRequestIds": [
                "downstream-request-1"
            ],
            "strandedConnectionScopedServerRequestMethods": [
                "item/tool/requestUserInput"
            ],
        })
    );
}

#[tokio::test]
async fn websocket_upgrade_closes_when_single_worker_disconnects_with_pending_connection_server_request()
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
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: None,
                                worker_websocket_url: test_worker_websocket_url(None),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            worker_id: None,
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
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
        STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_single_worker_session_ends_with_pending_connection_server_request()
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
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: None,
                                worker_websocket_url: test_worker_websocket_url(None),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            worker_id: None,
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
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
        STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_worker_cleanup_ids_when_single_worker_session_ends_with_pending_thread_scoped_server_request()
 {
    let websocket_url = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let observed_metrics = metrics.clone();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let websocket_url = websocket_url.clone();
            let metrics = metrics.clone();
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
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: None,
                                worker_websocket_url: test_worker_websocket_url(None),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
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
                            worker_id: None,
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("downstream_session_ended", false))
                    );
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            "serverRequest/resolved",
            serde_json::json!({
                "threadId": "thread-visible",
                "requestId": "gateway-srv-1",
            }),
        );

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::ERROR);
        assert_eq!(close_frame.reason, DOWNSTREAM_SESSION_ENDED_CLOSE_REASON);

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(
        logs.contains(
            "downstream app-server session ended with unresolved gateway server requests"
        )
    );
    assert!(logs.contains("worker_id=None"));
    assert!(logs.contains("remaining_worker_count=0"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=1"));
    assert!(logs.contains("resolved_thread_scoped_server_request_ids=[String(\"gateway-srv-1\")]"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=0"));
    assert_v2_server_request_lifecycle_metrics(
        &observed_metrics,
        &[
            (
                "worker_cleanup_resolved_thread_scoped",
                "item/tool/requestUserInput",
                1,
            ),
            (
                "worker_cleanup_resolution_delivered",
                "item/tool/requestUserInput",
                1,
            ),
        ],
    );
}

#[tokio::test]
async fn websocket_upgrade_resolves_unresolved_thread_scoped_server_request_when_single_worker_disconnects()
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
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: None,
                                worker_websocket_url: test_worker_websocket_url(None),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
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
                            worker_id: None,
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("downstream_session_ended", false))
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

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "gateway-srv-1",
        }),
    );

    let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::ERROR);
    assert_eq!(
        close_frame.reason,
        "downstream app-server disconnected: worker-a lost"
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_resolves_unresolved_thread_scoped_server_request_when_single_worker_session_ends()
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
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: None,
                                worker_websocket_url: test_worker_websocket_url(None),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
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
                            worker_id: None,
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("downstream_session_ended", false))
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

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "gateway-srv-1",
        }),
    );

    let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
        panic!("expected websocket close frame");
    };
    assert_eq!(u16::from(close_frame.code), close_code::ERROR);
    assert_eq!(close_frame.reason, DOWNSTREAM_SESSION_ENDED_CLOSE_REASON);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_worker_disconnects_with_unresolved_connection_server_request()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(0),
                                request_id: RequestId::String("downstream-request-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String("gateway-srv-1".to_string()),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            worker_id: Some(0),
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
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
        STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_resolves_unresolved_thread_scoped_server_request_when_worker_disconnects()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(0),
                                request_id: RequestId::String("downstream-request-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String("gateway-srv-1".to_string()),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
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
                            worker_id: Some(0),
                            event: Some(AppServerEvent::Disconnected {
                                message: "worker-a lost".to_string(),
                            }),
                        },
                    )
                    .await
                    .expect("disconnect event should be handled");
                    assert_eq!(should_close.is_none(), true);
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "gateway-srv-1",
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_closes_when_worker_session_ends_with_unresolved_connection_server_request()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(0),
                                request_id: RequestId::String("downstream-request-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String("gateway-srv-1".to_string()),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            worker_id: Some(0),
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
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
        STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_logs_worker_cleanup_ids_when_worker_session_ends_with_answered_but_unresolved_connection_server_request()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let (operator_events_tx, mut operator_events_rx) = broadcast::channel(4);
    let expected_worker_a_url = worker_a.clone();
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            let operator_events_tx = operator_events_tx.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability =
                        GatewayObservability::default().with_operator_events(operator_events_tx);
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(0),
                                request_id: RequestId::String("downstream-request-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String("gateway-srv-1".to_string()),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            worker_id: Some(0),
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
                    );
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::ERROR);
        assert_eq!(
            close_frame.reason,
            STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains("downstream worker session ended within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(0)"));
    assert!(logs.contains("remaining_worker_count=1"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=0"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=1"));
    assert!(
        logs.contains("stranded_connection_scoped_server_request_ids=[String(\"gateway-srv-1\")]")
    );
    assert!(logs.contains(
            "stranded_connection_scoped_downstream_server_request_ids=[String(\"downstream-request-1\")]"
        ));
    let cleanup_event = operator_events_rx
        .try_recv()
        .expect("worker cleanup event should publish");
    assert_eq!(cleanup_event.method, "gateway/v2ServerRequestCleanup");
    assert_eq!(
        cleanup_event.data,
        serde_json::json!({
            "workerId": 0,
            "workerWebsocketUrl": expected_worker_a_url,
            "remainingWorkerCount": 1,
            "disconnectMessage": null,
            "resolvedThreadScopedServerRequestCount": 0,
            "resolvedThreadScopedServerRequestIds": [],
            "resolvedThreadScopedDownstreamServerRequestIds": [],
            "resolvedThreadScopedServerRequestMethods": [],
            "resolvedThreadScopedThreadIds": [],
            "strandedConnectionScopedServerRequestCount": 1,
            "strandedConnectionScopedServerRequestIds": ["gateway-srv-1"],
            "strandedConnectionScopedDownstreamServerRequestIds": [
                "downstream-request-1"
            ],
            "strandedConnectionScopedServerRequestMethods": [
                "item/tool/requestUserInput"
            ],
        })
    );
}

#[tokio::test]
async fn websocket_upgrade_logs_worker_cleanup_ids_when_worker_session_ends_with_pending_connection_server_request()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::from([(
                            RequestId::String("gateway-srv-1".to_string()),
                            PendingServerRequestRoute {
                                worker_id: Some(0),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
                                downstream_request_id: RequestId::String(
                                    "downstream-request-1".to_string(),
                                ),
                                method: "item/tool/requestUserInput".to_string(),
                                thread_id: None,
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
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(
                        should_close
                            .map(|close| (close.outcome, close.reject_pending_server_requests)),
                        Some(("stranded_connection_scoped_server_request", true))
                    );
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let logs = capture_logs_async(async move {
        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        let Message::Close(Some(close_frame)) = wait_for_close_frame(&mut websocket).await else {
            panic!("expected websocket close frame");
        };
        assert_eq!(u16::from(close_frame.code), close_code::ERROR);
        assert_eq!(
            close_frame.reason,
            STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON
        );

        server_task.abort();
        let _ = server_task.await;
    })
    .await;

    assert!(logs.contains("downstream worker session ended within shared gateway v2 session"));
    assert!(logs.contains("worker_id=Some(0)"));
    assert!(logs.contains("remaining_worker_count=1"));
    assert!(logs.contains("resolved_thread_scoped_server_request_count=0"));
    assert!(logs.contains("stranded_connection_scoped_server_request_count=1"));
    assert!(
        logs.contains("stranded_connection_scoped_server_request_ids=[String(\"gateway-srv-1\")]")
    );
}

#[tokio::test]
async fn websocket_upgrade_resolves_unresolved_thread_scoped_server_request_when_worker_session_ends()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let app = Router::new().route(
        "/",
        any(move |websocket: WebSocketUpgrade| {
            let worker_a = worker_a.clone();
            let worker_b = worker_b.clone();
            async move {
                websocket.on_upgrade(move |mut socket| async move {
                    let admission = GatewayAdmissionController::default();
                    let observability = GatewayObservability::default();
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
                    let session_factory = GatewayV2SessionFactory::remote_multi(
                        vec![
                            RemoteAppServerConnectArgs {
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_a,
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
                                endpoint:
                                    codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                                        websocket_url: worker_b,
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
                    );
                    let initialize_params = InitializeParams {
                        client_info: ClientInfo {
                            name: "codex-tui".to_string(),
                            title: None,
                            version: "0.0.0-test".to_string(),
                        },
                        capabilities: None,
                    };
                    let mut router = GatewayV2DownstreamRouter::connect(
                        &session_factory,
                        &initialize_params,
                        &request_context,
                    )
                    .await
                    .expect("downstream router should connect");
                    let mut event_state = GatewayV2EventState {
                        pending_server_requests: HashMap::new(),
                        resolved_server_requests: HashMap::from([(
                            DownstreamServerRequestKey {
                                worker_id: Some(0),
                                request_id: RequestId::String("downstream-request-1".to_string()),
                            },
                            ResolvedServerRequestRoute {
                                gateway_request_id: RequestId::String("gateway-srv-1".to_string()),
                                worker_websocket_url: test_worker_websocket_url(Some(0)),
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
                            worker_id: Some(0),
                            event: None,
                        },
                    )
                    .await
                    .expect("session end event should be handled");
                    assert_eq!(should_close.is_none(), true);
                })
            }
        }),
    );
    let server = axum::serve(listener, app);
    let server_task = tokio::spawn(server.into_future());

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "serverRequest/resolved",
        serde_json::json!({
            "threadId": "thread-visible",
            "requestId": "gateway-srv-1",
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

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
