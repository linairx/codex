use super::*;
use pretty_assertions::assert_eq;
#[tokio::test]
async fn websocket_upgrade_forwards_bootstrap_setup_discovery_requests() {
    let cases = vec![
        (
            "externalAgentConfig/detect",
            "external-agent-config-detect",
            Some(serde_json::json!({
                "includeHome": true,
                "cwds": ["/tmp/project"],
            })),
            serde_json::json!({
                "items": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import CLAUDE.md from /tmp/project",
                    "cwd": "/tmp/project",
                    "details": null,
                }],
            }),
        ),
        (
            "app/list",
            "app-list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 25,
                "threadId": null,
            })),
            serde_json::json!({
                "data": [{
                    "id": "calendar",
                    "name": "Calendar",
                    "description": "Calendar connector",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
        ),
        (
            "skills/list",
            "skills-list",
            Some(serde_json::json!({
                "cwds": ["/tmp/project"],
                "forceReload": true,
                "perCwdExtraUserRoots": null,
            })),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/project",
                    "skills": [{
                        "name": "gateway-skill",
                        "description": "Gateway passthrough skill",
                        "shortDescription": "Gateway skill",
                        "interface": null,
                        "dependencies": null,
                        "path": "/tmp/project/.codex/skills/gateway-skill/SKILL.md",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
        ),
        (
            "plugin/list",
            "plugin-list",
            Some(serde_json::json!({
                "cwds": ["/tmp/project"],
            })),
            serde_json::json!({
                "marketplaces": [{
                    "name": "demo-marketplace",
                    "path": "/tmp/project/plugins/demo-marketplace.json",
                    "interface": {
                        "displayName": "Demo Marketplace",
                    },
                    "plugins": [{
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
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [],
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
        let (addr, server_task) = spawn_remote_gateway_v2_test_server(
            websocket_url,
            Arc::new(GatewayScopeRegistry::default()),
        )
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params,
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

#[tokio::test]
async fn websocket_upgrade_forwards_plugin_and_setup_mutation_requests() {
    let cases = vec![
        (
            "externalAgentConfig/import",
            "external-agent-config-import",
            Some(serde_json::json!({
                "migrationItems": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import CLAUDE.md from /tmp/project",
                    "cwd": "/tmp/project",
                    "details": null,
                }],
            })),
            serde_json::json!({}),
        ),
        (
            "plugin/read",
            "plugin-read",
            Some(serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            })),
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
            "plugin-install",
            Some(serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            })),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            "plugin-uninstall",
            Some(serde_json::json!({
                "pluginId": "demo-plugin@local",
            })),
            serde_json::json!({}),
        ),
        (
            "config/batchWrite",
            "config-batch-write",
            Some(serde_json::json!({
                "edits": [],
                "filePath": null,
                "expectedVersion": null,
                "reloadUserConfig": true,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "remote-version-2",
                "filePath": "/tmp/project/config.toml",
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
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
        let (addr, server_task) = spawn_remote_gateway_v2_test_server(
            websocket_url,
            Arc::new(GatewayScopeRegistry::default()),
        )
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params,
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

#[tokio::test]
async fn websocket_upgrade_forwards_core_thread_workflow_requests() {
    let cases = vec![
        (
            "thread/start",
            "thread-start",
            None,
            None,
            Some(serde_json::json!({
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "baseInstructions": null,
                "config": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "developerInstructions": null,
                "dynamicTools": null,
                "ephemeral": true,
                "experimentalRawEvents": false,
                "mockExperimentalField": null,
                "persistExtendedHistory": false,
                "personality": null,
                "sandbox": null,
                "serviceName": null,
                "sessionStartSource": null,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-started",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/resume",
            "thread-resume",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "history": null,
                "path": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "sandbox": null,
                "config": null,
                "baseInstructions": null,
                "developerInstructions": null,
                "personality": null,
                "persistExtendedHistory": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-resumed",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/fork",
            "thread-fork",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "path": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/project",
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "sandbox": null,
                "config": null,
                "baseInstructions": null,
                "developerInstructions": null,
                "ephemeral": true,
                "persistExtendedHistory": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-forked",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project",
                "instructionSources": [],
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandbox": { "type": "dangerFullAccess" },
                "reasoningEffort": null,
            }),
        ),
        (
            "thread/list",
            "thread-list",
            None,
            Some("thread-visible"),
            Some(serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": 10,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            })),
            serde_json::json!({
                "data": [{
                    "id": "thread-visible",
                    "name": "Visible thread",
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            "thread-loaded-list",
            None,
            Some("thread-visible"),
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": ["thread-visible"],
                "nextCursor": null,
            }),
        ),
        (
            "thread/read",
            "thread-read",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "includeTurns": false,
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
                    "name": "Visible thread",
                },
            }),
        ),
        (
            "thread/name/set",
            "thread-name-set",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "name": "Renamed thread",
            })),
            serde_json::json!({}),
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "mode": "enabled",
            })),
            serde_json::json!({}),
        ),
        (
            "turn/start",
            "turn-start",
            Some("thread-visible"),
            None,
            Some(serde_json::json!({
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "collaborationMode": null,
                "input": [],
                "cwd": "/tmp/project",
                "effort": null,
                "model": "gpt-5",
                "outputSchema": null,
                "personality": null,
                "responsesapiClientMetadata": null,
                "sandboxPolicy": null,
                "summary": null,
                "threadId": "thread-visible",
            })),
            serde_json::json!({
                "turn": {
                    "id": "turn-1",
                },
            }),
        ),
    ];

    for (method, request_id, thread_id, pre_registered_thread_id, params, result) in cases {
        let websocket_url =
            start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
                method,
                params.clone(),
                result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = thread_id {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        if let Some(thread_id) = pre_registered_thread_id {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
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
                    params,
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
