use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
pub(crate) async fn websocket_upgrade_fans_in_multi_worker_raw_response_items() {
    let metrics = in_memory_metrics();
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![(
        "rawResponseItem/completed",
        serde_json::json!({
            "threadId": "thread-worker-a",
            "turnId": "turn-worker-a",
            "item": {
                "type": "other",
            },
        }),
    )])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications(vec![(
        "rawResponseItem/completed",
        serde_json::json!({
            "threadId": "thread-worker-b",
            "turnId": "turn-worker-b",
            "item": {
                "type": "other",
            },
        }),
    )])
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
    scope_registry.register_thread("thread-worker-b".to_string(), context);
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry,
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

    let mut forwarded = Vec::new();
    for _ in 0..2 {
        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected raw response item notification");
        };
        forwarded.push(notification.params);
    }
    forwarded.sort_by_key(|params| {
        params
            .as_ref()
            .and_then(|params| params.get("threadId"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });

    assert_eq!(
        forwarded,
        vec![
            Some(serde_json::json!({
                "threadId": "thread-worker-a",
                "turnId": "turn-worker-a",
                "item": {
                    "type": "other",
                },
            })),
            Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-worker-b",
                "item": {
                    "type": "other",
                },
            })),
        ]
    );
    assert_v2_forwarded_notification_metric(&metrics, "rawResponseItem/completed", 2);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
pub(crate) async fn websocket_upgrade_fans_in_multi_worker_filesystem_changed_notifications() {
    let worker_a_params = serde_json::json!({
        "watchId": "watch-a",
        "changedPaths": ["/tmp/codex-gateway-worker-a-change.txt"],
    });
    let worker_b_params = serde_json::json!({
        "watchId": "watch-b",
        "changedPaths": ["/tmp/codex-gateway-worker-b-change.txt"],
    });
    let worker_a =
        start_mock_remote_server_for_connection_notification("fs/changed", worker_a_params).await;
    let worker_b =
        start_mock_remote_server_for_connection_notification("fs/changed", worker_b_params).await;
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

    let mut changed_paths = Vec::new();
    for _ in 0..2 {
        let JSONRPCMessage::Notification(notification) =
            read_websocket_message(&mut websocket).await
        else {
            panic!("expected fs/changed notification");
        };
        assert_eq!(notification.method, "fs/changed");
        changed_paths.push(
            notification
                .params
                .and_then(|params| params.get("changedPaths").cloned())
                .expect("fs/changed should include changedPaths"),
        );
    }
    changed_paths.sort_by_key(std::string::ToString::to_string);
    assert_eq!(
        changed_paths,
        vec![
            serde_json::json!(["/tmp/codex-gateway-worker-a-change.txt"]),
            serde_json::json!(["/tmp/codex-gateway-worker-b-change.txt"]),
        ]
    );

    server_task.abort();
    let _ = server_task.await;
}
