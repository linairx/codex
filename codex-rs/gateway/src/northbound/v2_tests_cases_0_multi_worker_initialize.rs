use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_initialize_capabilities_to_all_multi_worker_sessions() {
    let recorded_initialize_params = Arc::new(Mutex::new(Vec::new()));
    let worker_a =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let worker_b =
        start_mock_remote_server_recording_initialize(recorded_initialize_params.clone()).await;
    let initialize_response = test_initialize_response().await;
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
                    channel_capacity: 8,
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
                    channel_capacity: 8,
                },
            ],
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize_with_capabilities(
        &mut websocket,
        Some(InitializeCapabilities {
            request_attestation: false,
            experimental_api: true,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Some(vec![
                "thread/started".to_string(),
                "item/agentMessage/delta".to_string(),
            ]),
        }),
    )
    .await;

    let expected_initialize = InitializeParams {
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
    timeout(Duration::from_secs(5), async {
        loop {
            let ready = {
                let recorded = recorded_initialize_params.lock().await;
                recorded.len() == 2
            };
            if ready {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both workers should receive initialize params");
    assert_eq!(
        *recorded_initialize_params.lock().await,
        vec![expected_initialize.clone(), expected_initialize]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_initialized_notification_to_multi_worker_sessions() {
    let worker_a = start_mock_remote_server_expecting_forwarded_initialized().await;
    let worker_b = start_mock_remote_server_expecting_forwarded_initialized().await;
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Notification(JSONRPCNotification {
                method: "initialized".to_string(),
                params: None,
            }))
            .expect("initialized notification should serialize")
            .into(),
        ))
        .await
        .expect("initialized notification should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_chatgpt_auth_tokens_login_start_to_multi_worker_sessions() {
    let (worker_a_request_tx, worker_a_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let (worker_b_request_tx, worker_b_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_request_tx,
        serde_json::json!({
            "type": "chatgptAuthTokens",
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_request_tx,
        serde_json::json!({
            "type": "chatgptAuthTokens",
        }),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "chatgptAuthTokens",
                    "accessToken": "access-token-1",
                    "chatgptAccountId": "acct-123",
                    "chatgptPlanType": "pro",
                })),
                trace: None,
            }))
            .expect("login request should serialize")
            .into(),
        ))
        .await
        .expect("login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({ "type": "chatgptAuthTokens" })
    );

    let worker_a_request = worker_a_request_rx
        .await
        .expect("worker A should receive login request");
    let worker_b_request = worker_b_request_rx
        .await
        .expect("worker B should receive login request");
    assert_eq!(worker_a_request.method, "account/login/start");
    assert_eq!(worker_b_request.method, "account/login/start");
    assert_eq!(worker_a_request.params, worker_b_request.params);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fanouts_api_key_login_start_to_multi_worker_sessions() {
    let (worker_a_request_tx, worker_a_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let (worker_b_request_tx, worker_b_request_rx) = oneshot::channel::<JSONRPCRequest>();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_request_tx,
        serde_json::json!({
            "type": "apiKey",
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_request_tx,
        serde_json::json!({
            "type": "apiKey",
        }),
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "apiKey",
                    "apiKey": "sk-test-123",
                })),
                trace: None,
            }))
            .expect("login request should serialize")
            .into(),
        ))
        .await
        .expect("login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(response.result, serde_json::json!({ "type": "apiKey" }));

    let worker_a_request = worker_a_request_rx
        .await
        .expect("worker A should receive login request");
    let worker_b_request = worker_b_request_rx
        .await
        .expect("worker B should receive login request");
    assert_eq!(worker_a_request.method, "account/login/start");
    assert_eq!(worker_b_request.method, "account/login/start");
    assert_eq!(worker_a_request.params, worker_b_request.params);

    server_task.abort();
    let _ = server_task.await;
}
