use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_capability_requests() {
    let cases = vec![
        (
            "experimentalFeature/list",
            "experimental-feature-list",
            serde_json::json!({
                "cursor": null,
                "limit": 20,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-feature",
                    "stage": "beta",
                    "displayName": "Worker A Feature",
                    "description": "From worker A",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-feature",
                    "stage": "beta",
                    "displayName": "Worker B Feature",
                    "description": "From worker B",
                    "announcement": null,
                    "enabled": true,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-feature",
                        "stage": "beta",
                        "displayName": "Worker A Feature",
                        "description": "From worker A",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": false,
                    },
                    {
                        "name": "worker-b-feature",
                        "stage": "beta",
                        "displayName": "Worker B Feature",
                        "description": "From worker B",
                        "announcement": null,
                        "enabled": true,
                        "defaultEnabled": false,
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "collaborationMode/list",
            "collaboration-mode-list",
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-default",
                    "mode": "default",
                    "model": "gpt-5-worker-a",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-default",
                    "mode": "plan",
                    "model": "gpt-5-worker-b",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-default",
                        "mode": "default",
                        "model": "gpt-5-worker-a",
                        "reasoning_effort": null,
                    },
                    {
                        "name": "worker-b-default",
                        "mode": "plan",
                        "model": "gpt-5-worker-b",
                        "reasoning_effort": null,
                    }
                ],
            }),
        ),
    ];

    for (
        method,
        request_id,
        northbound_params,
        downstream_params,
        worker_a_result,
        worker_b_result,
        expected_result,
    ) in cases
    {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                downstream_params,
                worker_b_result,
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
                    params: Some(northbound_params),
                    trace: None,
                }))
                .expect("capability request should serialize")
                .into(),
            ))
            .await
            .expect("capability request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("capability response should arrive") else {
            panic!("expected capability response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_bootstrap_requests() {
    let cases = vec![
        (
            "account/read",
            "account-read",
            serde_json::json!({
                "refreshToken": false,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-a@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": false,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-b@example.com",
                    "planType": "enterprise",
                },
                "requiresOpenaiAuth": true,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-a@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": true,
            }),
        ),
        (
            "getAuthStatus",
            "get-auth-status",
            serde_json::json!({
                "includeToken": true,
                "refreshToken": false,
            }),
            serde_json::json!({
                "authMethod": "chatgpt",
                "authToken": "primary-token",
                "requiresOpenaiAuth": false,
            }),
            serde_json::json!({
                "authMethod": null,
                "authToken": null,
                "requiresOpenaiAuth": true,
            }),
            serde_json::json!({
                "authMethod": "chatgpt",
                "authToken": "primary-token",
                "requiresOpenaiAuth": true,
            }),
        ),
        (
            "account/rateLimits/read",
            "account-rate-limits-read",
            serde_json::json!({}),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 20,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": 20,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_000,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "worker-b",
                    "limitName": "Worker B",
                    "primary": {
                        "usedPercent": 35,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_500,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "worker-b": {
                        "limitId": "worker-b",
                        "limitName": "Worker B",
                        "primary": {
                            "usedPercent": 35,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_500,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 20,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": 20,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_000,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    },
                    "worker-b": {
                        "limitId": "worker-b",
                        "limitName": "Worker B",
                        "primary": {
                            "usedPercent": 35,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_500,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
        ),
        (
            "model/list",
            "model-list",
            serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": true,
            }),
            serde_json::json!({
                "data": [reconnectable_model_json("worker-a-model", "Worker A Model", true)],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [reconnectable_model_json("worker-b-model", "Worker B Model", false)],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-a-model", "Worker A Model", true),
                    reconnectable_model_json("worker-b-model", "Worker B Model", false),
                ],
                "nextCursor": null,
            }),
        ),
        (
            "externalAgentConfig/detect",
            "external-agent-config-detect",
            serde_json::json!({
                "includeHome": true,
                "cwds": ["/tmp/project"],
            }),
            serde_json::json!({
                "items": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import AGENTS.md from /tmp/worker-a",
                    "cwd": "/tmp/worker-a",
                    "details": null,
                }],
            }),
            serde_json::json!({
                "items": [{
                    "itemType": "CONFIG",
                    "description": "Import config from /tmp/worker-b",
                    "cwd": "/tmp/worker-b",
                    "details": null,
                }],
            }),
            serde_json::json!({
                "items": [
                    {
                        "itemType": "AGENTS_MD",
                        "description": "Import AGENTS.md from /tmp/worker-a",
                        "cwd": "/tmp/worker-a",
                        "details": null,
                    },
                    {
                        "itemType": "CONFIG",
                        "description": "Import config from /tmp/worker-b",
                        "cwd": "/tmp/worker-b",
                        "details": null,
                    }
                ],
            }),
        ),
        (
            "skills/list",
            "skills-list",
            serde_json::json!({
                "cwds": ["/tmp/project"],
                "perCwdExtraUserRoots": null,
            }),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
            serde_json::json!({
                "data": [
                    {
                        "cwd": "/tmp/worker-a",
                        "skills": [{
                            "name": "skill-a",
                            "description": "skill-a description",
                            "path": "/tmp/worker-a/skill-a",
                            "scope": "repo",
                            "enabled": true,
                        }],
                        "errors": [],
                    },
                    {
                        "cwd": "/tmp/worker-b",
                        "skills": [{
                            "name": "skill-b",
                            "description": "skill-b description",
                            "path": "/tmp/worker-b/skill-b",
                            "scope": "repo",
                            "enabled": true,
                        }],
                        "errors": [],
                    }
                ],
            }),
        ),
        (
            "app/list",
            "app-list",
            serde_json::json!({
                "cursor": null,
                "limit": 25,
                "threadId": null,
            }),
            serde_json::json!({
                "data": [{
                    "id": "worker-a-app",
                    "name": "Worker A App",
                    "description": "Worker A App description",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "id": "worker-b-app",
                    "name": "Worker B App",
                    "description": "Worker B App description",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "id": "worker-a-app",
                        "name": "Worker A App",
                        "description": "Worker A App description",
                        "installUrl": null,
                        "needsAuth": false,
                    },
                    {
                        "id": "worker-b-app",
                        "name": "Worker B App",
                        "description": "Worker B App description",
                        "installUrl": null,
                        "needsAuth": false,
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "plugin/list",
            "plugin-list",
            serde_json::json!({
                "cwds": ["/tmp/project"],
            }),
            serde_json::json!({
                "marketplaces": [{
                    "name": "worker-a-marketplace",
                    "path": "/tmp/project/worker-a-marketplace.json",
                    "interface": {
                        "displayName": "Worker A Marketplace",
                    },
                    "plugins": [{
                        "id": "worker-a-plugin@local",
                        "name": "worker-a-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/worker-a-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Worker A Plugin",
                            "shortDescription": "Worker A plugin",
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
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": ["worker-a-plugin@local"],
            }),
            serde_json::json!({
                "marketplaces": [{
                    "name": "worker-b-marketplace",
                    "path": "/tmp/project/worker-b-marketplace.json",
                    "interface": {
                        "displayName": "Worker B Marketplace",
                    },
                    "plugins": [{
                        "id": "worker-b-plugin@local",
                        "name": "worker-b-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/worker-b-plugin",
                        },
                        "installed": true,
                        "enabled": true,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Worker B Plugin",
                            "shortDescription": "Worker B plugin",
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
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": ["worker-b-plugin@local"],
            }),
            serde_json::json!({
                "marketplaces": [
                    {
                        "name": "worker-a-marketplace",
                        "path": "/tmp/project/worker-a-marketplace.json",
                        "interface": {
                            "displayName": "Worker A Marketplace",
                        },
                        "plugins": [{
                            "id": "worker-a-plugin@local",
                            "name": "worker-a-plugin",
                            "source": {
                                "type": "local",
                                "path": "/tmp/project/worker-a-plugin",
                            },
                            "installed": false,
                            "enabled": false,
                            "installPolicy": "AVAILABLE",
                            "authPolicy": "ON_USE",
                            "interface": {
                                "displayName": "Worker A Plugin",
                                "shortDescription": "Worker A plugin",
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
                                "logoDark": null,
                                "logoUrl": null,
                                "logoUrlDark": null,
                                "screenshots": [],
                                "screenshotUrls": [],
                            },
                        }],
                    },
                    {
                        "name": "worker-b-marketplace",
                        "path": "/tmp/project/worker-b-marketplace.json",
                        "interface": {
                            "displayName": "Worker B Marketplace",
                        },
                        "plugins": [{
                            "id": "worker-b-plugin@local",
                            "name": "worker-b-plugin",
                            "source": {
                                "type": "local",
                                "path": "/tmp/project/worker-b-plugin",
                            },
                            "installed": true,
                            "enabled": true,
                            "installPolicy": "AVAILABLE",
                            "authPolicy": "ON_USE",
                            "interface": {
                                "displayName": "Worker B Plugin",
                                "shortDescription": "Worker B plugin",
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
                                "logoDark": null,
                                "logoUrl": null,
                                "logoUrlDark": null,
                                "screenshots": [],
                                "screenshotUrls": [],
                            },
                        }],
                    }
                ],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [
                    "worker-a-plugin@local",
                    "worker-b-plugin@local",
                ],
            }),
        ),
        (
            "mcpServerStatus/list",
            "mcp-server-status-list",
            serde_json::json!({
                "cursor": null,
                "limit": 25,
                "detail": "toolsAndAuthOnly",
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-mcp",
                    "tools": {},
                    "resources": [],
                    "resourceTemplates": [],
                    "authStatus": "bearerToken",
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-mcp",
                    "tools": {},
                    "resources": [],
                    "resourceTemplates": [],
                    "authStatus": "bearerToken",
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-mcp",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "bearerToken",
                    },
                    {
                        "name": "worker-b-mcp",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "bearerToken",
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "thread/realtime/listVoices",
            "realtime-list-voices",
            serde_json::json!({}),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper", "maple"],
                    "v2": ["alloy"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
            serde_json::json!({
                "voices": {
                    "v1": ["maple", "cove"],
                    "v2": ["alloy", "marin"],
                    "defaultV1": "cove",
                    "defaultV2": "marin",
                },
            }),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper", "maple", "cove"],
                    "v2": ["alloy", "marin"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
        ),
        (
            "fuzzyFileSearch",
            "fuzzy-file-search",
            serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-reconnected",
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/worker-a",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 40,
                    "indices": [5, 6, 7, 8],
                }],
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/worker-b",
                    "path": "src/gateway.rs",
                    "match_type": "file",
                    "file_name": "gateway.rs",
                    "score": 60,
                    "indices": [4, 5, 6, 7],
                }],
            }),
            serde_json::json!({
                "files": [
                    {
                        "root": "/tmp/worker-b",
                        "path": "src/gateway.rs",
                        "match_type": "file",
                        "file_name": "gateway.rs",
                        "score": 60,
                        "indices": [4, 5, 6, 7],
                    },
                    {
                        "root": "/tmp/worker-a",
                        "path": "docs/gateway.md",
                        "match_type": "file",
                        "file_name": "gateway.md",
                        "score": 40,
                        "indices": [5, 6, 7, 8],
                    }
                ],
            }),
        ),
    ];

    for (method, request_id, params, worker_a_result, worker_b_result, expected_result) in cases {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
                start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
                    method,
                    match method {
                        "account/rateLimits/read" => None,
                        "app/list" => Some(serde_json::json!({
                            "cursor": null,
                            "limit": null,
                            "threadId": null,
                        })),
                        "mcpServerStatus/list" => Some(serde_json::json!({
                            "cursor": null,
                            "limit": null,
                            "detail": "toolsAndAuthOnly",
                        })),
                        _ => Some(params.clone()),
                    },
                    worker_b_result,
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
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("bootstrap request should serialize")
                .into(),
            ))
            .await
            .expect("bootstrap request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("bootstrap response should arrive") else {
            panic!("expected bootstrap response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        match method {
            "account/rateLimits/read" => {
                let response: GetAccountRateLimitsResponse =
                    serde_json::from_value(response.result)
                        .expect("rate limits response should decode");
                assert_eq!(response.rate_limits.limit_id.as_deref(), Some("codex"));
                assert_eq!(response.rate_limits.limit_name.as_deref(), Some("Codex"));
                assert_eq!(
                    response.rate_limits_by_limit_id.as_ref().map(HashMap::len),
                    Some(2)
                );
                assert_eq!(
                    response
                        .rate_limits_by_limit_id
                        .as_ref()
                        .and_then(|limits| limits.get("worker-b"))
                        .and_then(|limits| limits.limit_name.as_deref()),
                    Some("Worker B")
                );
            }
            "app/list" => {
                let mut response: AppsListResponse = serde_json::from_value(response.result)
                    .expect("app/list response should decode");
                response.data.sort_by(|a, b| a.id.cmp(&b.id));
                assert_eq!(response.next_cursor, None);
                assert_eq!(response.data.len(), 2);
                assert_eq!(response.data[0].id, "worker-a-app");
                assert_eq!(response.data[0].name, "Worker A App");
                assert_eq!(response.data[1].id, "worker-b-app");
                assert_eq!(response.data[1].name, "Worker B App");
            }
            _ => {
                let mut actual = response.result;
                let mut expected_result = expected_result;
                canonicalize_bootstrap_response_json(&mut actual);
                canonicalize_bootstrap_response_json(&mut expected_result);
                assert_eq!(actual, expected_result);
            }
        }

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_thread_requests() {
    let cases = vec![
        (
            "thread/list",
            "thread-list",
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": 10,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": null,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-a",
                    "Worker A thread",
                    "/tmp/worker-a",
                    1,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-b",
                    "Worker B thread",
                    "/tmp/worker-b",
                    2,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            "thread-loaded-list",
            serde_json::json!({
                "cursor": null,
                "limit": 10,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-a"],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-b"],
                "nextCursor": null,
            }),
        ),
    ];

    for (
        method,
        request_id,
        northbound_params,
        downstream_params,
        worker_a_result,
        worker_b_result,
    ) in cases
    {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                downstream_params,
                worker_b_result,
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
        scope_registry.register_thread("thread-worker-b".to_string(), context.clone());
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
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
                    params: Some(northbound_params.clone()),
                    trace: None,
                }))
                .expect("thread aggregation request should serialize")
                .into(),
            ))
            .await
            .expect("thread aggregation request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("thread aggregation response should arrive") else {
            panic!("expected thread aggregation response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));

        match method {
            "thread/list" => {
                let listed: ThreadListResponse = serde_json::from_value(response.result)
                    .expect("thread/list response should decode");
                assert_eq!(listed.next_cursor, None);
                assert_eq!(listed.backwards_cursor, None);
                assert_eq!(
                    listed
                        .data
                        .iter()
                        .map(|thread| thread.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["thread-worker-b", "thread-worker-a"]
                );
            }
            "thread/loaded/list" => {
                let loaded: ThreadLoadedListResponse = serde_json::from_value(response.result)
                    .expect("thread/loaded/list response should decode");
                assert_eq!(loaded.next_cursor, None);
                assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);
            }
            other => panic!("unexpected thread aggregation method: {other}"),
        }

        assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_sticky_thread_requests() {
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            serde_json::json!({}),
        ),
        (
            "turn/steer",
            "turn-steer",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "expectedTurnId": "turn-reconnected",
                "input": [{
                    "type": "text",
                    "text": "continue",
                }],
            }),
            serde_json::json!({}),
        ),
        (
            "turn/interrupt",
            "turn-interrupt",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-reconnected",
            }),
            serde_json::json!({}),
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
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
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
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky thread request should serialize")
                .into(),
            ))
            .await
            .expect("sticky thread request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky thread response should arrive") else {
            panic!("expected sticky thread response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_additional_sticky_thread_controls() {
    let cases = vec![
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
        ),
        (
            "thread/archive",
            "thread-archive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/unarchive",
            "thread-unarchive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
            }),
        ),
        (
            "thread/metadata/update",
            "thread-metadata-update",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "gitInfo": {
                    "sha": "abc123",
                    "branch": "main",
                    "originUrl": null,
                },
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "gitInfo": {
                        "commitHash": "abc123",
                        "branchName": "main",
                        "remoteUrl": null,
                    },
                },
            }),
        ),
        (
            "thread/turns/list",
            "thread-turns-list",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "cursor": null,
                "limit": 20,
                "sortDirection": "desc",
            }),
            serde_json::json!({
                "data": [{
                    "id": "turn-1",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": 1,
                    "completedAt": 2,
                    "durationMs": 1,
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/increment_elicitation",
            "thread-increment-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 2,
                "paused": true,
            }),
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 1,
                "paused": false,
            }),
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            }),
            serde_json::json!({}),
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/shellCommand",
            "thread-shell-command",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "command": "git status --short",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/backgroundTerminals/clean",
            "thread-background-terminals-clean",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/rollback",
            "thread-rollback",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "numTurns": 2,
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
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
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
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
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky thread control request should serialize")
                .into(),
            ))
            .await
            .expect("sticky thread control request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky thread control response should arrive") else {
            panic!("expected sticky thread control response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_review_start_and_backfills_review_thread_route()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b =
        start_mock_remote_server_for_disconnect_then_reconnectable_review_start_then_thread_read()
            .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::clone(&scope_registry),
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
                id: RequestId::String("review-start".to_string()),
                method: "review/start".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "target": {
                        "type": "custom",
                        "instructions": "Review the current change",
                    },
                    "delivery": "detached",
                })),
                trace: None,
            }))
            .expect("review/start request should serialize")
            .into(),
        ))
        .await
        .expect("review/start request should send");

    let JSONRPCMessage::Response(review_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("review/start response should arrive") else {
        panic!("expected review/start response");
    };
    assert_eq!(
        review_response.id,
        RequestId::String("review-start".to_string())
    );
    assert_eq!(
        review_response.result,
        serde_json::json!({
            "turn": {
                "id": "turn-review",
                "items": [],
                "status": "pending",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null,
            },
            "reviewThreadId": "thread-review",
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-read-review".to_string()),
                method: "thread/read".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-review",
                    "includeTurns": false,
                })),
                trace: None,
            }))
            .expect("thread/read request should serialize")
            .into(),
        ))
        .await
        .expect("thread/read request should send");

    let JSONRPCMessage::Response(thread_read_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("thread/read response should arrive") else {
        panic!("expected thread/read response");
    };
    assert_eq!(
        thread_read_response.id,
        RequestId::String("thread-read-review".to_string())
    );
    assert_eq!(
        thread_read_response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-review",
                "name": "Detached review thread",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_sticky_realtime_requests() {
    let cases = vec![
        (
            "thread/realtime/start",
            "realtime-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "outputModality": "text",
                "transport": {
                    "type": "websocket"
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendText",
            "realtime-append-text",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "text": "hello realtime",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendAudio",
            "realtime-append-audio",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-visible",
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/stop",
            "realtime-stop",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
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
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
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
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky realtime request should serialize")
                .into(),
            ))
            .await
            .expect("sticky realtime request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky realtime response should arrive") else {
            panic!("expected sticky realtime response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_turn_start() {
    let expected_result = serde_json::json!({
        "turn": {
            "id": "turn-reconnected",
        },
    });
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::clone(&scope_registry),
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
                id: RequestId::String("turn-start".to_string()),
                method: "turn/start".to_string(),
                params: Some(serde_json::json!({
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "collaborationMode": null,
                    "input": [],
                    "cwd": "/tmp/worker-b",
                    "effort": null,
                    "model": "gpt-5",
                    "outputSchema": null,
                    "personality": null,
                    "responsesapiClientMetadata": null,
                    "sandboxPolicy": null,
                    "summary": null,
                    "threadId": "thread-worker-b",
                })),
                trace: None,
            }))
            .expect("turn/start request should serialize")
            .into(),
        ))
        .await
        .expect("turn/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("turn/start response should arrive") else {
        panic!("expected turn/start response");
    };
    assert_eq!(response.id, RequestId::String("turn-start".to_string()));
    assert_eq!(response.result, expected_result);

    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["turn/start".to_string()]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_primary_worker_requests() {
    let cases = vec![
        (
            "configRequirements/read",
            "config-requirements-read",
            None,
            serde_json::json!({
                "requirements": [],
                "validationErrors": [],
            }),
        ),
        (
            "account/login/start",
            "account-login-start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-reconnected",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            "account-login-cancel",
            Some(serde_json::json!({
                "loginId": "login-reconnected",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "command/exec",
            "command-exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                "processId": "proc-reconnected",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            "command-exec-write",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            "command-exec-resize",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            "command-exec-terminate",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "feedback/upload",
            "feedback-upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway reconnect regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-reconnected",
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            "fuzzy-file-search-session-start",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            "fuzzy-file-search-session-update",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            "fuzzy-file-search-session-stop",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            "windows-sandbox-setup-start",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
            }),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                serde_json::json!({
                    "worker": "unexpected-secondary",
                }),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = params
            .as_ref()
            .and_then(|value| value.get("threadId"))
            .and_then(Value::as_str)
        {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
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
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: params.clone(),
                    trace: None,
                }))
                .expect("primary-worker request should serialize")
                .into(),
            ))
            .await
            .expect("primary-worker request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("primary-worker response should arrive") else {
            panic!("expected primary-worker response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);
        assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());

        server_task.abort();
        let _ = server_task.await;
    }
}
