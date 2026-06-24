use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_4_notifications.rs"]
mod v2_tests_cases_4_notifications;

#[path = "v2_tests_cases_4_realtime.rs"]
mod v2_tests_cases_4_realtime;

#[path = "v2_tests_cases_4_thread_notifications.rs"]
mod v2_tests_cases_4_thread_notifications;

#[path = "v2_tests_cases_4_realtime_notifications.rs"]
mod v2_tests_cases_4_realtime_notifications;

#[path = "v2_tests_cases_4_late.rs"]
mod v2_tests_cases_4_late;

#[path = "v2_tests_cases_4_thread_start.rs"]
mod v2_tests_cases_4_thread_start;

#[path = "v2_tests_cases_4_thread_start_tail_account_capacity.rs"]
mod v2_tests_cases_4_thread_start_tail_account_capacity;

#[path = "v2_tests_cases_4_thread_start_tail.rs"]
mod v2_tests_cases_4_thread_start_tail;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_4_late::*;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_fanout_setup_mutations() {
    let cases = vec![
        (
            "externalAgentConfig/import",
            "external-agent-config-import",
            Some(serde_json::json!({
                "migrationItems": [],
            })),
            serde_json::json!({}),
        ),
        (
            "config/batchWrite",
            "config-batch-write",
            Some(serde_json::json!({
                "edits": [],
                "filePath": "/tmp/shared/config.toml",
                "expectedVersion": null,
                "reloadUserConfig": true,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        (
            "config/value/write",
            "config-value-write",
            Some(serde_json::json!({
                "keyPath": "plugins.shared-plugin",
                "value": {
                    "enabled": true,
                },
                "mergeStrategy": "upsert",
                "filePath": null,
                "expectedVersion": null,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
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
        (
            "account/login/start",
            "account-login-start-api-key",
            Some(serde_json::json!({
                "type": "apiKey",
                "apiKey": "sk-test",
            })),
            serde_json::json!({
                "type": "apiKey",
                "accountId": "acct-worker-a",
            }),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: params.clone(),
                    trace: None,
                }))
                .expect("fanout setup mutation request should serialize")
                .into(),
            ))
            .await
            .expect("fanout setup mutation request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("fanout setup mutation response should arrive") else {
            panic!("expected fanout setup mutation response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);
        assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_fs_watch_and_unwatch() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_and_unwatch().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("fs-watch".to_string()),
                method: "fs/watch".to_string(),
                params: Some(serde_json::json!({
                    "watchId": "watch-shared",
                    "path": "/tmp/shared/project/.git/HEAD",
                })),
                trace: None,
            }))
            .expect("fs/watch request should serialize")
            .into(),
        ))
        .await
        .expect("fs/watch request should send");

    let JSONRPCMessage::Response(watch_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("fs/watch response should arrive") else {
        panic!("expected fs/watch response");
    };
    assert_eq!(watch_response.id, RequestId::String("fs-watch".to_string()));
    let watch_result: FsWatchResponse =
        serde_json::from_value(watch_response.result).expect("fs/watch response should decode");
    assert_eq!(
        watch_result.path.as_ref().to_string_lossy(),
        "/tmp/shared/project/.git/HEAD"
    );
    assert_eq!(
        *worker_a_requests.lock().await,
        vec!["fs/watch".to_string()]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["fs/watch".to_string()]
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("fs-unwatch".to_string()),
                method: "fs/unwatch".to_string(),
                params: Some(serde_json::json!({
                    "watchId": "watch-shared",
                })),
                trace: None,
            }))
            .expect("fs/unwatch request should serialize")
            .into(),
        ))
        .await
        .expect("fs/unwatch request should send");

    let JSONRPCMessage::Response(unwatch_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("fs/unwatch response should arrive") else {
        panic!("expected fs/unwatch response");
    };
    assert_eq!(
        unwatch_response.id,
        RequestId::String("fs-unwatch".to_string())
    );
    let _: FsUnwatchResponse =
        serde_json::from_value(unwatch_response.result).expect("fs/unwatch response should decode");
    assert_eq!(
        *worker_a_requests.lock().await,
        vec!["fs/watch".to_string(), "fs/unwatch".to_string()]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["fs/watch".to_string(), "fs/unwatch".to_string()]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_fs_changed_from_reconnected_worker_after_fs_watch() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_fs_watch_with_changed_notification(
            )
            .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
            vec![
                RemoteAppServerConnectArgs {
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
                    endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("fs-watch".to_string()),
                method: "fs/watch".to_string(),
                params: Some(serde_json::json!({
                    "watchId": "watch-reconnected",
                    "path": "/tmp/shared/project/.git/HEAD",
                })),
                trace: None,
            }))
            .expect("fs/watch request should serialize")
            .into(),
        ))
        .await
        .expect("fs/watch request should send");

    let mut saw_watch_response = false;
    let mut saw_changed_notification = false;
    for _ in 0..2 {
        match timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("fs/watch response or fs/changed notification should arrive")
        {
            JSONRPCMessage::Response(response) => {
                assert_eq!(response.id, RequestId::String("fs-watch".to_string()));
                let watch_result: FsWatchResponse = serde_json::from_value(response.result)
                    .expect("fs/watch response should decode");
                assert_eq!(
                    watch_result.path.as_ref().to_string_lossy(),
                    "/tmp/shared/project/.git/HEAD"
                );
                saw_watch_response = true;
            }
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "fs/changed");
                assert_eq!(
                    notification.params,
                    Some(serde_json::json!({
                        "watchId": "watch-reconnected",
                        "changedPaths": ["/tmp/shared/project/.git/HEAD"],
                    }))
                );
                saw_changed_notification = true;
            }
            message => panic!("unexpected gateway message: {message:?}"),
        }
    }

    assert_eq!(saw_watch_response, true);
    assert_eq!(saw_changed_notification, true);
    assert_eq!(
        *worker_a_requests.lock().await,
        vec!["fs/watch".to_string()]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["fs/watch".to_string()]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn reconnect_missing_workers_replays_initialized_and_active_fs_watches() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let context = GatewayRequestContext::default();
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    assert_eq!(router.worker_count(), 2);

    router.mark_initialized();
    router.record_fs_watch(FsWatchParams {
        watch_id: "watch-shared".to_string(),
        path: PathBuf::from("/tmp/shared/project/.git/HEAD")
            .try_into()
            .expect("fs watch path should be absolute"),
    });

    assert!(
        router.remove_worker(Some(0)),
        "test should drop the primary worker before reconnect"
    );
    assert_eq!(router.worker_count(), 1);

    timeout(
        Duration::from_secs(2),
        router.reconnect_missing_workers(&GatewayObservability::default()),
    )
    .await
    .expect("worker reconnect should finish in time");
    assert_eq!(router.worker_count(), 2);

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
}

#[tokio::test]
async fn downstream_router_emits_disconnect_only_once_per_worker_session() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let context = GatewayRequestContext::default();
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");

    let disconnect_event = timeout(Duration::from_secs(2), router.next_event())
        .await
        .expect("disconnect event should finish in time")
        .expect("disconnect event should exist");
    assert_eq!(disconnect_event.worker_id, Some(0));
    let Some(AppServerEvent::Disconnected { message }) = disconnect_event.event else {
        panic!("expected a terminal disconnect event from the first worker");
    };
    assert_eq!(message.is_empty(), false);

    assert!(
        timeout(Duration::from_millis(200), router.next_event())
            .await
            .is_err(),
        "terminal disconnect should not be followed by a duplicate worker end-of-stream event"
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_logs_attempt_and_success_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
                            websocket_url: worker_b.clone(),
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
            ),
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
        }),
    };

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(
        logs.contains("attempting to reconnect missing downstream worker session"),
        "{logs}"
    );
    assert!(
        logs.contains("reconnected missing downstream worker session"),
        "{logs}"
    );
    assert!(logs.contains("worker_id=1"), "{logs}");
    assert!(
        logs.contains(format!("websocket_url=\"{worker_b}\"").as_str()),
        "{logs}"
    );
    assert!(
        logs.contains("initialized_notification_sent=false"),
        "{logs}"
    );
    assert!(logs.contains("active_fs_watch_count=0"), "{logs}");
    assert!(logs.contains("retry_backoff_seconds=1"), "{logs}");

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_logs_connect_failure_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_a = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![worker_a, "ws://127.0.0.1:1".to_string()],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://127.0.0.1:9".to_string(),
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
                            websocket_url: "ws://127.0.0.1:1".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(7),
        }),
    };

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("attempting to reconnect missing downstream worker session"));
    assert!(logs.contains("failed to reconnect missing downstream worker session"));
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("websocket_url=\"ws://127.0.0.1:1\""));
    assert!(logs.contains("initialized_notification_sent=true"));
    assert!(logs.contains("active_fs_watch_count=1"));
    assert!(logs.contains("retry_backoff_seconds=7"));

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_logs_replay_failure_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_a = start_mock_remote_server_for_initialize().await;
    let replay_failure_worker =
        start_mock_remote_server_for_reconnectable_initialized_replay_failure().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![worker_a, replay_failure_worker.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://127.0.0.1:9".to_string(),
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
                            websocket_url: replay_failure_worker.clone(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(5),
        }),
    };

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("attempting to reconnect missing downstream worker session"));
    assert!(
        logs.contains("failed to replay connection state to reconnected downstream worker session")
    );
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains(format!("websocket_url=\"{replay_failure_worker}\"").as_str()));
    assert!(logs.contains("initialized_notification_sent=true"));
    assert!(logs.contains("active_fs_watch_count=1"));
    assert!(logs.contains("retry_backoff_seconds=5"));

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_success_metrics() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 2);
    assert_v2_worker_reconnect_metrics(&metrics, &[(1, "attempt"), (1, "success")]);

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_reuses_initialize_capabilities() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let recorded_initialize_params = Arc::new(Mutex::new(Vec::new()));
    let worker_b =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec![
                "thread/started".to_string(),
                "item/agentMessage/delta".to_string(),
            ]),
        }),
    };
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
            ),
            initialize_params: initialize_params.clone(),
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
        .await;

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        *recorded_initialize_params.lock().await,
        vec![initialize_params]
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_connect_failure_metrics() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://127.0.0.1:1".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
                            websocket_url: "ws://127.0.0.1:1".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 1);
    assert_v2_worker_reconnect_metrics(
        &metrics,
        &[
            (1, "attempt"),
            (1, "attempt"),
            (1, "attempt"),
            (1, "connect_failure"),
            (1, "connect_failure"),
            (1, "connect_failure"),
        ],
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_downstream_protocol_violation_metrics() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_that_sends_invalid_jsonrpc_during_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    let logs = capture_logs_async(async {
        router
            .reconnect_missing_workers_at(Instant::now(), &observability, true)
            .await;
        assert_eq!(router.worker_count(), 1);
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("sent invalid initialize response"), "{logs}");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let resource_metrics = metrics.snapshot().expect("snapshot");
    let metrics = resource_metrics
        .scope_metrics()
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics);
    let mut saw_protocol_violation = false;
    let mut saw_worker_reconnect_attempt = false;
    let mut saw_worker_reconnect_failure = false;
    for metric in metrics {
        match metric.name() {
            "gateway_v2_protocol_violations" => match metric.data() {
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
                                    ("phase".to_string(), "downstream".to_string()),
                                    ("reason".to_string(), "invalid_jsonrpc".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 1);
                                saw_protocol_violation = true;
                            }
                        }
                    }
                    _ => panic!("unexpected protocol violation count aggregation"),
                },
                _ => panic!("unexpected protocol violation count type"),
            },
            "gateway_v2_worker_reconnects" => match metric.data() {
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
                                    ("worker_id".to_string(), "1".to_string()),
                                    ("outcome".to_string(), "attempt".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 3);
                                saw_worker_reconnect_attempt = true;
                            }
                            if attributes
                                == BTreeMap::from([
                                    ("worker_id".to_string(), "1".to_string()),
                                    ("outcome".to_string(), "connect_failure".to_string()),
                                ])
                            {
                                assert_eq!(point.value(), 3);
                                saw_worker_reconnect_failure = true;
                            }
                        }
                    }
                    _ => panic!("unexpected worker reconnect count aggregation"),
                },
                _ => panic!("unexpected worker reconnect count type"),
            },
            _ => {}
        }
    }
    assert!(saw_protocol_violation);
    assert!(saw_worker_reconnect_attempt);
    assert!(saw_worker_reconnect_failure);

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_records_attempt_and_replay_failure_metrics() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let replay_failure_worker = start_mock_remote_server_that_disconnects_after_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                replay_failure_worker.clone(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
                            websocket_url: replay_failure_worker,
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };

    router
        .reconnect_missing_workers_at(Instant::now(), &observability, true)
        .await;

    assert_eq!(router.worker_count(), 1);
    assert_v2_worker_reconnect_metrics(
        &metrics,
        &[
            (1, "attempt"),
            (1, "attempt"),
            (1, "attempt"),
            (1, "replay_failure"),
        ],
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
    client.shutdown().await.expect("client should shut down");
}

#[test]
fn worker_reconnect_backoff_suppresses_immediate_retry_attempts() {
    let (event_tx, event_rx) = mpsc::channel(1);
    let now = Instant::now();
    let retry_backoff = Duration::from_secs(3);
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

    assert!(router.should_attempt_worker_reconnect(0, now));
    assert!(router.should_attempt_worker_reconnect(1, now));

    router.record_worker_reconnect_failure(0, now, retry_backoff);
    assert!(!router.should_attempt_worker_reconnect(0, now));
    assert!(!router.should_attempt_worker_reconnect(0, now + Duration::from_secs(2)));
    assert!(router.should_attempt_worker_reconnect(0, now + retry_backoff));
    assert!(router.should_attempt_worker_reconnect(1, now));

    router.clear_worker_reconnect_failure(0);
    assert!(router.should_attempt_worker_reconnect(0, now + Duration::from_secs(1)));
}

#[tokio::test]
async fn reconnect_missing_workers_records_backoff_suppressed_metric() {
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
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let now = Instant::now();
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(3),
        }),
    };
    router.record_worker_reconnect_failure(1, now, Duration::from_secs(3));

    let logs = capture_logs_async(async {
        router
            .reconnect_missing_workers_at(now, &observability, true)
            .await;

        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert_v2_worker_reconnect_metric(&metrics, 1, "backoff_suppressed");
    assert!(
        logs.contains(
            "suppressing missing downstream worker reconnect while retry backoff is active"
        )
    );
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("websocket_url=\"ws://worker-b.invalid\""));
    assert!(logs.contains("initialized_notification_sent=false"));
    assert!(logs.contains("active_fs_watch_count=0"));
    assert!(logs.contains("retry_backoff_seconds=3"));
    assert!(logs.contains("reconnect_backoff_remaining_seconds="));
    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn primary_worker_requests_stay_fail_closed_with_one_live_worker_in_multi_worker_topology() {
    let (client, request_handle) = start_test_request_handle().await;
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
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(1),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
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
        }),
    };

    let err = match super::super::super::worker_for_request(
        &mut router,
        &GatewayScopeRegistry::default(),
        &GatewayRequestContext::default(),
        &JSONRPCRequest {
            id: RequestId::String("command-exec".to_string()),
            method: "command/exec".to_string(),
            params: Some(serde_json::json!({
                "command": ["pwd"],
                "tty": false,
                "streamStdin": false,
                "streamStdoutStderr": false,
            })),
            trace: None,
        },
    ) {
        Ok(_) => panic!("primary-worker request should stay fail-closed"),
        Err(err) => err,
    };

    assert_eq!(
        err.to_string(),
        "primary worker route is unavailable for command/exec"
    );

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn thread_scoped_requests_stay_sticky_with_one_live_worker_in_multi_worker_topology() {
    let (client, request_handle) = start_test_request_handle().await;
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
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(1),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
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
        }),
    };
    let scope_registry = GatewayScopeRegistry::default();
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let worker = super::super::super::worker_for_request(
        &mut router,
        &scope_registry,
        &context,
        &JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .expect("thread-scoped request should stay sticky to live worker");
    assert_eq!(worker.worker_id, Some(1));

    let worker = super::super::super::worker_for_notification(
        &mut router,
        &scope_registry,
        &JSONRPCNotification {
            method: "thread/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
            })),
        },
    )
    .expect("thread-scoped notification should stay sticky to live worker");
    assert_eq!(worker.worker_id, Some(1));

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn thread_scoped_requests_fail_closed_when_account_capacity_is_exhausted() {
    let (client_a, request_handle_a) = start_test_request_handle().await;
    let (client_b, request_handle_b) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://worker-a.invalid".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://worker-b.invalid".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());
    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![
            DownstreamWorkerHandle {
                worker_id: Some(0),
                worker_websocket_url: None,
                request_handle: request_handle_a,
            },
            DownstreamWorkerHandle {
                worker_id: Some(1),
                worker_websocket_url: None,
                request_handle: request_handle_b,
            },
        ],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
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
        }),
    };
    let scope_registry = GatewayScopeRegistry::default();
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(32);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let admission = GatewayAdmissionController::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let cases = [
        (
            "turn/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "prompt": "continue",
            }),
        ),
        (
            "turn/steer",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-1",
                "message": "adjust course",
            }),
        ),
        (
            "turn/interrupt",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-1",
            }),
        ),
        (
            "app/list",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "cursor": null,
                "limit": 20,
            }),
        ),
        (
            "thread/unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/compact/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/shellCommand",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "command": "cargo test",
            }),
        ),
        (
            "thread/backgroundTerminals/clean",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/realtime/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/realtime/appendText",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "text": "hello",
            }),
        ),
        (
            "thread/realtime/appendAudio",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": "AA==",
            }),
        ),
        (
            "thread/realtime/stop",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "mcpServer/resource/read",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "server": "filesystem",
                "uri": "file:///tmp/notes.txt",
            }),
        ),
        (
            "mcpServer/tool/call",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "server": "filesystem",
                "tool": "read_file",
                "arguments": {},
            }),
        ),
        (
            "review/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
    ];
    for (method, params) in cases {
        let err = match super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(method.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        {
            Ok(_) => {
                panic!("{method} should fail closed instead of moving active context")
            }
            Err(err) => err,
        };

        let uses_short_message = method.starts_with("turn/")
            || method == "app/list"
            || method == "review/start"
            || method == "thread/shellCommand"
            || method == "thread/backgroundTerminals/clean"
            || method.starts_with("thread/realtime/")
            || method.starts_with("mcpServer/");

        let expected_message = if uses_short_message {
            format!(
                "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}"
            )
        } else {
            format!(
                "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}, and no replacement worker restored the context"
            )
        };
        assert_eq!(err.to_string(), expected_message);
        let handoff_event = operator_events_rx
            .recv()
            .await
            .expect("active thread handoff failure event should be published");
        assert_eq!(
            handoff_event.method,
            if uses_short_message {
                "gateway/accountActiveThreadHandoffFailed"
            } else {
                "gateway/accountThreadHandoffFailed"
            }
        );
        assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
        assert_eq!(
            handoff_event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": method,
                "threadId": "thread-worker-b",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "reason": expected_message,
            })
        );
    }
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [
            ("active_thread_handoff_failure".to_string(), 13),
            ("thread_compact_start_handoff_failure".to_string(), 1),
            ("thread_unsubscribe_handoff_failure".to_string(), 1),
        ]
        .into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [
                ("active_thread_handoff_failure".to_string(), 13),
                ("thread_compact_start_handoff_failure".to_string(), 1),
                ("thread_unsubscribe_handoff_failure".to_string(), 1),
            ]
            .into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for review/start"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    assert_v2_account_capacity_event_metric_count(&metrics, 1, "active_thread_handoff_failure", 13);
    client_a
        .shutdown()
        .await
        .expect("client A should shut down");
    client_b
        .shutdown()
        .await
        .expect("client B should shut down");
}
