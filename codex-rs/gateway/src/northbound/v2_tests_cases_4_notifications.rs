use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_filesystem_changed_notifications() {
    let params = serde_json::json!({
        "watchId": "watch-1",
        "changedPaths": ["/tmp/codex-gateway-fs-change.txt"],
    });
    let initialize_response = test_initialize_response().await;
    let websocket_url =
        start_mock_remote_server_for_connection_notification("fs/changed", params.clone()).await;
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

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "fs/changed",
        params,
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_fuzzy_file_search_notifications() {
    let search_result = FuzzyFileSearchResult {
        root: "/tmp/project".to_string(),
        path: "docs/gateway.md".to_string(),
        match_type: FuzzyFileSearchMatchType::File,
        file_name: "gateway.md".to_string(),
        score: 42,
        indices: Some(vec![5, 6, 7, 8]),
    };
    let cases = vec![
        ServerNotification::FuzzyFileSearchSessionUpdated(
            FuzzyFileSearchSessionUpdatedNotification {
                session_id: "search-session-1".to_string(),
                query: "gate".to_string(),
                files: vec![search_result],
            },
        ),
        ServerNotification::FuzzyFileSearchSessionCompleted(
            FuzzyFileSearchSessionCompletedNotification {
                session_id: "search-session-1".to_string(),
            },
        ),
    ];

    for notification in cases {
        let initialize_response = test_initialize_response().await;
        let expected =
            tagged_type_to_notification(&notification).expect("notification should serialize");
        let websocket_url = start_mock_remote_server_for_notification(notification).await;
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

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Notification(actual) = message else {
            panic!("expected fuzzy file search notification, got {message:?}");
        };
        assert_eq!(actual, expected);

        server_task.abort();
        let _ = server_task.await;
    }
}
