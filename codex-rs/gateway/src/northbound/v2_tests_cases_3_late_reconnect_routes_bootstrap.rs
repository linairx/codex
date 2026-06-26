use super::*;
use pretty_assertions::assert_eq;

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
