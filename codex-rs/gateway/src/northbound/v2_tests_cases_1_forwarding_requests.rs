use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_fuzzy_file_search_requests() {
    let cases = vec![
        (
            "fuzzyFileSearch",
            "fuzzy-file-search",
            serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-1",
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/project",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 42,
                    "indices": [5, 6, 7, 8],
                }],
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            "fuzzy-file-search-session-start",
            serde_json::json!({
                "sessionId": "search-session-1",
                "roots": ["/tmp/project"],
            }),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            "fuzzy-file-search-session-update",
            serde_json::json!({
                "sessionId": "search-session-1",
                "query": "gate",
            }),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            "fuzzy-file-search-session-stop",
            serde_json::json!({
                "sessionId": "search-session-1",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("fuzzy file search request should serialize")
                .into(),
            ))
            .await
            .expect("fuzzy file search request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected fuzzy file search response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_filesystem_operation_requests() {
    let cases = vec![
        (
            "fs/readFile",
            "fs-read-file",
            serde_json::json!({
                "path": "/tmp/project/input.txt",
            }),
            serde_json::json!({
                "dataBase64": "Z2F0ZXdheS1maWxl",
            }),
        ),
        (
            "fs/writeFile",
            "fs-write-file",
            serde_json::json!({
                "path": "/tmp/project/output.txt",
                "dataBase64": "Z2F0ZXdheS13cml0ZQ==",
            }),
            serde_json::json!({}),
        ),
        (
            "fs/createDirectory",
            "fs-create-directory",
            serde_json::json!({
                "path": "/tmp/project/nested",
                "recursive": true,
            }),
            serde_json::json!({}),
        ),
        (
            "fs/getMetadata",
            "fs-get-metadata",
            serde_json::json!({
                "path": "/tmp/project/output.txt",
            }),
            serde_json::json!({
                "isDirectory": false,
                "isFile": true,
                "isSymlink": false,
                "createdAtMs": 0,
                "modifiedAtMs": 0,
            }),
        ),
        (
            "fs/readDirectory",
            "fs-read-directory",
            serde_json::json!({
                "path": "/tmp/project",
            }),
            serde_json::json!({
                "entries": [{
                    "fileName": "output.txt",
                    "isDirectory": false,
                    "isFile": true,
                }],
            }),
        ),
        (
            "fs/copy",
            "fs-copy",
            serde_json::json!({
                "sourcePath": "/tmp/project/output.txt",
                "destinationPath": "/tmp/project/copy.txt",
            }),
            serde_json::json!({}),
        ),
        (
            "fs/remove",
            "fs-remove",
            serde_json::json!({
                "path": "/tmp/project/copy.txt",
                "recursive": true,
                "force": true,
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let initialize_response = test_initialize_response().await;
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("filesystem request should serialize")
                .into(),
            ))
            .await
            .expect("filesystem request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected filesystem response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_config_value_write_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "config/value/write",
        serde_json::json!({
            "keyPath": "plugins.demo-plugin",
            "value": {
                "enabled": true,
            },
            "mergeStrategy": "upsert",
            "filePath": null,
            "expectedVersion": null,
        }),
        serde_json::json!({
            "status": "ok",
            "version": "remote-version-1",
            "filePath": "/tmp/remote-project/config.toml",
            "overriddenMetadata": null,
        }),
    )
    .await;
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("config-value-write".to_string()),
                method: "config/value/write".to_string(),
                params: Some(serde_json::json!({
                    "keyPath": "plugins.demo-plugin",
                    "value": {
                        "enabled": true,
                    },
                    "mergeStrategy": "upsert",
                    "filePath": null,
                    "expectedVersion": null,
                })),
                trace: None,
            }))
            .expect("config value write request should serialize")
            .into(),
        ))
        .await
        .expect("config value write request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected config value write response");
    };
    assert_eq!(
        response.id,
        RequestId::String("config-value-write".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "status": "ok",
            "version": "remote-version-1",
            "filePath": "/tmp/remote-project/config.toml",
            "overriddenMetadata": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_model_list_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "model/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
            "includeHidden": true,
        }),
        serde_json::json!({
            "data": [
                {
                    "model": "gpt-5",
                    "provider": "openai",
                    "contextWindow": 272000,
                    "maxOutputTokens": 32000,
                    "supportsImages": true,
                    "supportsPromptCacheKey": true,
                    "supportsResponseSchema": true,
                    "supportsReasoningSummaries": true,
                    "supportsEncryptedReasoningContent": false,
                    "supportsReasoningEffort": true,
                    "supportsCustomToolCallInput": true,
                    "supportsParallelToolCalls": true,
                    "supportsToolChoiceRequired": true,
                    "supportsTerminalToolCall": true,
                    "supportsPreserveBackground": true,
                    "supportsMinimalEffortReasoning": true,
                    "supportsVerbosity": true,
                    "overrideRank": null,
                    "upgradeInfo": null,
                    "availabilityNux": null,
                    "displayName": "GPT-5",
                    "description": "Gateway test model",
                    "hidden": false,
                    "supportedReasoningEfforts": [
                        {
                            "reasoningEffort": "medium",
                            "description": "Balanced",
                        }
                    ],
                    "defaultReasoningEffort": "medium",
                    "inputModalities": ["text"],
                    "supportsPersonality": false,
                    "additionalSpeedTiers": [],
                    "isDefault": true,
                }
            ],
            "nextCursor": null,
        }),
    )
    .await;
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
            .expect("model list request should serialize")
            .into(),
        ))
        .await
        .expect("model list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected model list response");
    };
    assert_eq!(response.id, RequestId::String("model-list".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": [
                {
                    "model": "gpt-5",
                    "provider": "openai",
                    "contextWindow": 272000,
                    "maxOutputTokens": 32000,
                    "supportsImages": true,
                    "supportsPromptCacheKey": true,
                    "supportsResponseSchema": true,
                    "supportsReasoningSummaries": true,
                    "supportsEncryptedReasoningContent": false,
                    "supportsReasoningEffort": true,
                    "supportsCustomToolCallInput": true,
                    "supportsParallelToolCalls": true,
                    "supportsToolChoiceRequired": true,
                    "supportsTerminalToolCall": true,
                    "supportsPreserveBackground": true,
                    "supportsMinimalEffortReasoning": true,
                    "supportsVerbosity": true,
                    "overrideRank": null,
                    "upgradeInfo": null,
                    "availabilityNux": null,
                    "displayName": "GPT-5",
                    "description": "Gateway test model",
                    "hidden": false,
                    "supportedReasoningEfforts": [
                        {
                            "reasoningEffort": "medium",
                            "description": "Balanced",
                        }
                    ],
                    "defaultReasoningEffort": "medium",
                    "inputModalities": ["text"],
                    "supportsPersonality": false,
                    "additionalSpeedTiers": [],
                    "isDefault": true,
                }
            ],
            "nextCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_server_status_list_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "mcpServerStatus/list",
        serde_json::json!({
            "cursor": null,
            "limit": 100,
            "detail": "toolsAndAuthOnly",
        }),
        serde_json::json!({
            "data": [{
                "name": "calendar",
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken"
            }],
            "nextCursor": null,
        }),
    )
    .await;
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("mcp-status-list".to_string()),
                method: "mcpServerStatus/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 100,
                    "detail": "toolsAndAuthOnly",
                })),
                trace: None,
            }))
            .expect("mcp status list request should serialize")
            .into(),
        ))
        .await
        .expect("mcp status list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected mcp status list response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-status-list".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": [{
                "name": "calendar",
                "tools": {},
                "resources": [],
                "resourceTemplates": [],
                "authStatus": "bearerToken"
            }],
            "nextCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_server_oauth_login_requests() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "mcpServer/oauth/login",
        serde_json::json!({
            "name": "calendar",
            "scopes": ["calendar.read"],
            "timeoutSecs": 120,
        }),
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/calendar",
        }),
    )
    .await;
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

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("mcp-oauth-login".to_string()),
                method: "mcpServer/oauth/login".to_string(),
                params: Some(serde_json::json!({
                    "name": "calendar",
                    "scopes": ["calendar.read"],
                    "timeoutSecs": 120,
                })),
                trace: None,
            }))
            .expect("mcp oauth login request should serialize")
            .into(),
        ))
        .await
        .expect("mcp oauth login request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected mcp oauth login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-oauth-login".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/calendar",
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_additional_thread_control_requests() {
    let cases = vec![
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/shellCommand",
            "thread-shell-command",
            serde_json::json!({
                "threadId": "thread-visible",
                "command": "git status --short",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/backgroundTerminals/clean",
            "thread-background-terminals-clean",
            serde_json::json!({
                "threadId": "thread-visible",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/rollback",
            "thread-rollback",
            serde_json::json!({
                "threadId": "thread-visible",
                "numTurns": 2,
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
                    "name": "Visible thread",
                    "turns": [],
                },
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

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
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let message = read_websocket_message(&mut websocket).await;
        let JSONRPCMessage::Response(response) = message else {
            panic!("expected response for {method}, got {message:?}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}
