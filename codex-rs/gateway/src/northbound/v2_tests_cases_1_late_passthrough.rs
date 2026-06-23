use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_additional_low_frequency_passthrough_requests() {
    let cases = vec![
        (
            "command/exec",
            "command-exec",
            None,
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-command-exec"],
                "processId": "proc-visible",
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
            None,
            Some(serde_json::json!({
                "processId": "proc-visible",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            "command-exec-resize",
            None,
            Some(serde_json::json!({
                "processId": "proc-visible",
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
            None,
            Some(serde_json::json!({
                "processId": "proc-visible",
            })),
            serde_json::json!({}),
        ),
        (
            "thread/archive",
            "thread-archive",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
            })),
            serde_json::json!({}),
        ),
        (
            "thread/unarchive",
            "thread-unarchive",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
                    "name": "Visible thread",
                    "turns": [],
                },
            }),
        ),
        (
            "thread/metadata/update",
            "thread-metadata-update",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "gitInfo": {
                    "sha": "abc123",
                    "branch": "main",
                    "originUrl": null,
                },
            })),
            serde_json::json!({
                "thread": {
                    "id": "thread-visible",
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
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "cursor": null,
                "limit": 20,
                "sortDirection": "desc",
            })),
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
            "thread/realtime/listVoices",
            "thread-realtime-list-voices",
            None,
            Some(serde_json::json!({})),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper"],
                    "v2": ["alloy"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
        ),
        (
            "thread/increment_elicitation",
            "thread-increment-elicitation",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
            })),
            serde_json::json!({
                "count": 2,
                "paused": true,
            }),
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
            })),
            serde_json::json!({
                "count": 1,
                "paused": false,
            }),
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            })),
            serde_json::json!({}),
        ),
        (
            "config/read",
            "config-read",
            None,
            Some(serde_json::json!({
                "includeLayers": true,
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "config": {},
                "origins": {},
                "layers": null,
            }),
        ),
        (
            "configRequirements/read",
            "config-requirements-read",
            None,
            None,
            serde_json::json!({
                "requirements": null,
            }),
        ),
        (
            "experimentalFeature/list",
            "experimental-feature-list",
            None,
            Some(serde_json::json!({
                "cursor": null,
                "limit": 20,
            })),
            serde_json::json!({
                "data": [{
                    "name": "gateway-test-feature",
                    "stage": "beta",
                    "displayName": "Gateway Test Feature",
                    "description": "Used by gateway passthrough tests",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
        ),
        (
            "experimentalFeature/enablement/set",
            "experimental-feature-enablement-set",
            None,
            Some(serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            })),
            serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            }),
        ),
        (
            "collaborationMode/list",
            "collaboration-mode-list",
            None,
            Some(serde_json::json!({})),
            serde_json::json!({
                "data": [{
                    "name": "default",
                    "mode": "default",
                    "model": null,
                    "reasoningEffort": null,
                }],
            }),
        ),
        (
            "marketplace/add",
            "marketplace-add",
            None,
            Some(serde_json::json!({
                "source": "https://example.com/gateway-marketplace.git",
                "refName": null,
                "sparsePaths": null,
            })),
            serde_json::json!({
                "marketplaceName": "gateway-marketplace",
                "installedRoot": "/tmp/project/.codex/plugins/gateway-marketplace",
                "alreadyAdded": false,
            }),
        ),
        (
            "skills/config/write",
            "skills-config-write",
            None,
            Some(serde_json::json!({
                "path": "/tmp/project/.codex/skills/gateway-skill/SKILL.md",
                "name": null,
                "enabled": true,
            })),
            serde_json::json!({
                "effectiveEnabled": true,
            }),
        ),
        (
            "config/mcpServer/reload",
            "mcp-server-reload",
            None,
            None,
            serde_json::json!({}),
        ),
        (
            "mcpServer/resource/read",
            "mcp-resource-read",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "server": "gateway-mcp",
                "uri": "file:///tmp/project/context.txt",
            })),
            serde_json::json!({
                "contents": [],
            }),
        ),
        (
            "mcpServer/tool/call",
            "mcp-server-tool-call",
            Some("thread-visible"),
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "server": "gateway-mcp",
                "tool": "lookup",
                "arguments": {
                    "query": "gateway",
                },
            })),
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": "gateway tool result",
                }],
                "structuredContent": null,
                "isError": false,
                "_meta": null,
            }),
        ),
        (
            "account/sendAddCreditsNudgeEmail",
            "account-send-add-credits-nudge-email",
            None,
            Some(serde_json::json!({
                "creditType": "credits",
            })),
            serde_json::json!({
                "status": "sent",
            }),
        ),
        (
            "getAuthStatus",
            "get-auth-status",
            None,
            Some(serde_json::json!({
                "includeToken": true,
                "refreshToken": false,
            })),
            serde_json::json!({
                "authMethod": "chatgpt",
                "authToken": "legacy-token",
                "requiresOpenaiAuth": true,
            }),
        ),
        (
            "windowsSandbox/setupStart",
            "windows-sandbox-setup-start",
            None,
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
            }),
        ),
    ];

    for (method, request_id, thread_id, params, result) in cases {
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
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
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
