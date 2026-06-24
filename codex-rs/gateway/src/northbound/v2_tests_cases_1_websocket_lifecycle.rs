use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_1_websocket_lifecycle_disconnect.rs"]
mod v2_tests_cases_1_websocket_lifecycle_disconnect;

#[path = "v2_tests_cases_1_websocket_lifecycle_unknown_server_request.rs"]
mod v2_tests_cases_1_websocket_lifecycle_unknown_server_request;

#[path = "v2_tests_cases_1_websocket_lifecycle_invalid_payload.rs"]
mod v2_tests_cases_1_websocket_lifecycle_invalid_payload;

#[path = "v2_tests_cases_1_websocket_lifecycle_timeout.rs"]
mod v2_tests_cases_1_websocket_lifecycle_timeout;

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
