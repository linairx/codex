use super::*;
use pretty_assertions::assert_eq;

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
