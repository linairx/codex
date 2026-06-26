use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_forwarding_server_requests() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_thread_start_then_server_requests().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
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
    send_initialized(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-start".to_string()),
                method: "thread/start".to_string(),
                params: Some(serde_json::json!({
                    "model": null,
                    "modelProvider": null,
                    "serviceTier": null,
                    "cwd": "/tmp/recovered-worker",
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "sandbox": null,
                    "config": null,
                    "serviceName": null,
                    "baseInstructions": null,
                    "developerInstructions": null,
                    "personality": null,
                    "ephemeral": true,
                    "sessionStartSource": null,
                    "dynamicTools": null,
                    "experimentalRawEvents": false,
                    "persistExtendedHistory": false,
                })),
                trace: None,
            }))
            .expect("thread/start request should serialize")
            .into(),
        ))
        .await
        .expect("thread/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("thread/start response should arrive") else {
        panic!("expected thread/start response");
    };
    assert_eq!(response.id, RequestId::String("thread-start".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-recovered",
                "forkedFromId": null,
                "preview": "",
                "ephemeral": true,
                "modelProvider": "openai",
                "createdAt": 1,
                "updatedAt": 1,
                "status": {
                    "type": "idle",
                },
                "path": null,
                "cwd": "/tmp/recovered-worker",
                "cliVersion": "0.0.0-test",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": null,
                "turns": [],
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/recovered-worker",
            "instructionSources": [],
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "sandbox": {
                "type": "dangerFullAccess",
            },
            "reasoningEffort": null,
        })
    );

    let JSONRPCMessage::Request(user_input_request) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("forwarded user-input request should arrive") else {
        panic!("expected forwarded user-input request");
    };
    assert_eq!(user_input_request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        user_input_request.params,
        Some(serde_json::json!({
            "threadId": "thread-recovered",
            "turnId": "turn-recovered",
            "itemId": "tool-call-recovered",
            "questions": [{
                "id": "mode",
                "header": "Mode",
                "question": "Pick execution mode",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: user_input_request.id,
                result: serde_json::json!({
                    "answers": {
                        "mode": {
                            "answers": ["safe"],
                        },
                    },
                }),
            }))
            .expect("user-input response should serialize")
            .into(),
        ))
        .await
        .expect("user-input response should send");

    let JSONRPCMessage::Request(refresh_request) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("forwarded chatgpt refresh request should arrive") else {
        panic!("expected forwarded chatgpt refresh request");
    };
    assert_eq!(refresh_request.method, "account/chatgptAuthTokens/refresh");
    assert_json_params_eq(
        refresh_request.params,
        Some(serde_json::json!({
            "reason": "unauthorized",
            "previousAccountId": "acct-recovered",
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: refresh_request.id,
                result: serde_json::json!({
                    "accessToken": "access-token-recovered",
                    "chatgptAccountId": "acct-recovered",
                    "chatgptPlanType": "pro",
                }),
            }))
            .expect("refresh response should serialize")
            .into(),
        ))
        .await
        .expect("refresh response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_forwarding_login_completed() {
    let worker_a = start_mock_remote_server_for_reconnectable_primary_login_completed().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
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
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "chatgpt",
                })),
                trace: None,
            }))
            .expect("account/login/start request should serialize")
            .into(),
        ))
        .await
        .expect("account/login/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("account/login/start response should arrive") else {
        panic!("expected account/login/start response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "type": "chatgpt",
            "loginId": "login-reconnected",
            "authUrl": "https://example.com/login",
        })
    );

    assert_jsonrpc_notification(
        timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("account/login/completed notification should arrive"),
        "account/login/completed",
        serde_json::json!({
            "loginId": "login-reconnected",
            "success": true,
            "error": null,
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_forwarding_mcp_oauth_completed() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "mcpServer/oauth/login",
        serde_json::json!({
            "name": "shared-mcp",
        }),
        JSONRPCErrorError {
            code: crate::northbound::v2::INVALID_PARAMS_CODE,
            message: "mcpServer/oauth/login missing on worker-a".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_mcp_oauth_login_completed().await;
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
                id: RequestId::String("mcp-oauth-login".to_string()),
                method: "mcpServer/oauth/login".to_string(),
                params: Some(serde_json::json!({
                    "name": "shared-mcp",
                })),
                trace: None,
            }))
            .expect("mcpServer/oauth/login request should serialize")
            .into(),
        ))
        .await
        .expect("mcpServer/oauth/login request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("mcpServer/oauth/login response should arrive") else {
        panic!("expected mcpServer/oauth/login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-oauth-login".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "authorizationUrl": "https://example.com/oauth/shared-mcp",
        })
    );

    assert_jsonrpc_notification(
        timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("mcpServer/oauthLogin/completed notification should arrive"),
        "mcpServer/oauthLogin/completed",
        serde_json::json!({
            "name": "shared-mcp",
            "success": true,
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_startup_status_from_reconnected_worker() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_mcp_server_status_list(vec!["worker-a-mcp"])
            .await;
    let worker_b =
            start_mock_remote_server_for_disconnect_then_reconnectable_mcp_status_with_startup_notification(
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
                id: RequestId::String("mcp-status-list".to_string()),
                method: "mcpServerStatus/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 25,
                    "detail": "toolsAndAuthOnly",
                })),
                trace: None,
            }))
            .expect("mcpServerStatus/list request should serialize")
            .into(),
        ))
        .await
        .expect("mcpServerStatus/list request should send");

    let mut saw_status_response = false;
    let mut saw_startup_notification = false;
    for _ in 0..2 {
        match timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("mcp status response or startup notification should arrive")
        {
            JSONRPCMessage::Response(response) => {
                assert_eq!(
                    response.id,
                    RequestId::String("mcp-status-list".to_string())
                );
                let mut status: ListMcpServerStatusResponse =
                    serde_json::from_value(response.result)
                        .expect("mcp status response should decode");
                status.data.sort_by(|a, b| a.name.cmp(&b.name));
                assert_eq!(status.next_cursor, None);
                assert_eq!(status.data.len(), 2);
                assert_eq!(status.data[0].name, "worker-a-mcp");
                assert_eq!(status.data[1].name, "worker-b-mcp");
                saw_status_response = true;
            }
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "mcpServer/startupStatus/updated");
                assert_eq!(
                    notification.params,
                    Some(serde_json::json!({
                        "threadId": null,
                        "name": "worker-b-mcp",
                        "status": "ready",
                        "error": null,
                        "failureReason": null,
                    }))
                );
                saw_startup_notification = true;
            }
            message => panic!("unexpected gateway message: {message:?}"),
        }
    }

    assert_eq!(saw_status_response, true);
    assert_eq!(saw_startup_notification, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_plugin_management_fallback_requests() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: crate::northbound::v2::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                params.clone(),
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

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("plugin management request should serialize")
                .into(),
            ))
            .await
            .expect("plugin management request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("plugin management response should arrive") else {
            panic!("expected plugin management response for {method}");
        };
        assert_eq!(response.id, RequestId::String(format!("{method}-request")));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[path = "v2_tests_cases_3_late_reconnect_config.rs"]
mod v2_tests_cases_3_late_reconnect_config;

#[path = "v2_tests_cases_3_late_reconnect_routes.rs"]
mod v2_tests_cases_3_late_reconnect_routes;
