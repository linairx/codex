use super::*;
use pretty_assertions::assert_eq;

use crate::northbound::v2::DOWNSTREAM_SESSION_ENDED_CLOSE_REASON;
use crate::northbound::v2::STRANDED_CONNECTION_SERVER_REQUEST_CLOSE_REASON;

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
