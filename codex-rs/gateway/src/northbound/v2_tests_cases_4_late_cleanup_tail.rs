use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2::DOWNSTREAM_SESSION_ENDED_CLOSE_REASON;
use crate::northbound::v2::DUPLICATE_DOWNSTREAM_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2::STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON;
use crate::northbound::v2_connection::GatewayV2ReconnectState;

#[path = "v2_tests_cases_4_late_connection_cleanup.rs"]
mod v2_tests_cases_4_late_connection_cleanup;

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
