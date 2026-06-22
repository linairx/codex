use super::*;

pub(crate) async fn start_reconnecting_v2_bootstrap_refresh_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let mut plugin_installed = false;
        for connection_index in 0..3 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");

            expect_remote_initialize(&mut websocket).await;
            match connection_index {
                0 => {
                    websocket
                        .close(None)
                        .await
                        .expect("close frame should send");
                }
                1 => {
                    tokio::spawn(async move {
                        tokio::time::sleep(crate::embedded::REMOTE_WORKER_RECONNECT_DELAY).await;
                        drop(websocket);
                    });
                }
                2 => {
                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        match request.method.as_str() {
                            "account/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "account": {
                                                "type": "chatgpt",
                                                "email": "gateway@example.com",
                                                "planType": "pro",
                                            },
                                            "requiresOpenaiAuth": false,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "account/rateLimits/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "rateLimits": {
                                                "limitId": "codex",
                                                "limitName": "Codex",
                                                "primary": {
                                                    "usedPercent": 42,
                                                    "windowDurationMins": 300,
                                                    "resetsAt": 1_700_000_000,
                                                },
                                                "secondary": null,
                                                "credits": null,
                                                "planType": "pro",
                                                "rateLimitReachedType": null,
                                            },
                                            "rateLimitsByLimitId": {
                                                "codex": {
                                                    "limitId": "codex",
                                                    "limitName": "Codex",
                                                    "primary": {
                                                        "usedPercent": 42,
                                                        "windowDurationMins": 300,
                                                        "resetsAt": 1_700_000_000,
                                                    },
                                                    "secondary": null,
                                                    "credits": null,
                                                    "planType": "pro",
                                                    "rateLimitReachedType": null,
                                                }
                                            },
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "model/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "id": "openai/gpt-5",
                                                "model": "gpt-5",
                                                "upgrade": null,
                                                "upgradeInfo": null,
                                                "availabilityNux": null,
                                                "displayName": "GPT-5",
                                                "description": "Recovered single-worker model",
                                                "hidden": false,
                                                "supportedReasoningEfforts": [{
                                                    "reasoningEffort": "medium",
                                                    "description": "Balanced",
                                                }],
                                                "defaultReasoningEffort": "medium",
                                                "inputModalities": ["text"],
                                                "supportsPersonality": false,
                                                "additionalSpeedTiers": [],
                                                "isDefault": true,
                                            }],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "externalAgentConfig/detect" => {
                                assert_eq!(
                                    request.params,
                                    Some(serde_json::json!({
                                        "includeHome": true,
                                        "cwds": ["/tmp/reconnected-project"],
                                    })),
                                );
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "items": [{
                                                "itemType": "PLUGINS",
                                                "description": "reconnected repo config",
                                                "cwd": "/tmp/reconnected-project",
                                                "details": null,
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "app/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "id": "reconnected-app",
                                                "name": "Reconnected App",
                                                "description": "Recovered single-worker app",
                                                "installUrl": null,
                                                "displayName": "Recovered App",
                                                "shortDescription": "Recovered app list entry",
                                                "logoUrl": null,
                                                "bgColor": null,
                                                "createdAt": null,
                                                "deprecationReason": null,
                                            }],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: reconnected_plugin_list_response(plugin_installed),
                                    }),
                                )
                                .await;
                            }
                            "plugin/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: reconnected_plugin_read_response(plugin_installed),
                                    }),
                                )
                                .await;
                            }
                            "plugin/install" => {
                                plugin_installed = true;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "authPolicy": "ON_INSTALL",
                                            "appsNeedingAuth": [],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "plugin/uninstall" => {
                                plugin_installed = false;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                            }
                            "skills/list" => {
                                write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": [{
                                            "cwd": "/tmp/reconnected-project",
                                            "skills": [{
                                                "name": "reconnected-skill",
                                                "description": "Recovered single-worker skill",
                                                "path": "/tmp/reconnected-project/skills/reconnected-skill",
                                                "scope": "repo",
                                                "enabled": true,
                                            }],
                                            "errors": [],
                                        }],
                                    }),
                                }),
                            )
                            .await;
                            }
                            "config/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "config": {
                                                "model": "gpt-5",
                                            },
                                            "origins": {},
                                            "layers": [{
                                                "name": {
                                                    "type": "project",
                                                    "dotCodexFolder": "/tmp/reconnected-project",
                                                },
                                                "version": "reconnected-version-1",
                                                "config": {
                                                    "model": "gpt-5",
                                                },
                                                "disabledReason": null,
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "configRequirements/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "requirements": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "experimentalFeature/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "name": "gateway-test-feature",
                                                "stage": "beta",
                                                "displayName": "Gateway Test Feature",
                                                "description": "Used by gateway reconnect tests",
                                                "announcement": null,
                                                "enabled": false,
                                                "defaultEnabled": false,
                                            }],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "collaborationMode/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "name": "default",
                                                "mode": "default",
                                                "model": null,
                                                "reasoningEffort": null,
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "mcpServerStatus/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "name": "reconnected-mcp",
                                                "tools": {},
                                                "resources": [],
                                                "resourceTemplates": [],
                                                "authStatus": "bearerToken",
                                            }],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "externalAgentConfig/import" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "importId": "reconnected-import",
                                            "itemTypeResults": [],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "marketplace/add" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "marketplaceName": "reconnected-marketplace",
                                            "installedRoot": "/tmp/reconnected-project/marketplace",
                                            "alreadyAdded": false,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "skills/config/write" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "effectiveEnabled": true,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "experimentalFeature/enablement/set" => {
                                let enablement = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("enablement"))
                                .cloned()
                                .expect("experimentalFeature/enablement/set should include enablement");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "enablement": enablement,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "config/mcpServer/reload" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                            }
                            "config/batchWrite" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "status": "ok",
                                            "version": "reconnected-version-1",
                                            "filePath": "/tmp/reconnected-project/config.toml",
                                            "overriddenMetadata": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "config/value/write" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "status": "ok",
                                            "version": "reconnected-version-1",
                                            "filePath": "/tmp/reconnected-project/config.toml",
                                            "overriddenMetadata": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "memory/reset"
                            | "account/logout"
                            | "fs/createDirectory"
                            | "fs/writeFile"
                            | "fs/copy"
                            | "fs/remove"
                            | "fuzzyFileSearch/sessionStart"
                            | "command/exec/write"
                            | "command/exec/resize"
                            | "command/exec/terminate" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                            }
                            "feedback/upload" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "threadId": "feedback-thread-reconnected",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "account/sendAddCreditsNudgeEmail" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "status": "sent",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "fs/readFile" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "dataBase64": "cmVjb25uZWN0ZWQtZmlsZQ==",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "fs/getMetadata" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "isDirectory": false,
                                            "isFile": true,
                                            "isSymlink": false,
                                            "createdAtMs": 0,
                                            "modifiedAtMs": 0,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "fs/readDirectory" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "entries": [{
                                                "fileName": "gateway.txt",
                                                "isDirectory": false,
                                                "isFile": true,
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "fs/watch" => {
                                let (watch_id, path) = {
                                    let params = request
                                        .params
                                        .as_ref()
                                        .expect("fs/watch params should exist");
                                    let watch_id = params["watchId"]
                                        .as_str()
                                        .expect("watch id should be string")
                                        .to_string();
                                    let path = params["path"]
                                        .as_str()
                                        .expect("watch path should be string")
                                        .to_string();
                                    (watch_id, path)
                                };
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "path": path,
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fs/changed".to_string(),
                                            params: Some(serde_json::json!({
                                                "watchId": watch_id,
                                                "changedPaths": [
                                                    "/tmp/reconnected-project/config.toml"
                                                ],
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "fuzzyFileSearch" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "files": [{
                                                "root": "/tmp/reconnected-project",
                                                "path": "docs/reconnected.md",
                                                "match_type": "file",
                                                "file_name": "reconnected.md",
                                                "score": 100,
                                                "indices": null,
                                            }],
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "fuzzyFileSearch/sessionUpdate" => {
                                let (session_id, query) = {
                                    let params = request.params.as_ref().expect(
                                        "fuzzyFileSearch/sessionUpdate params should exist",
                                    );
                                    let session_id = params["sessionId"]
                                        .as_str()
                                        .expect("session id should be string")
                                        .to_string();
                                    let query = params["query"]
                                        .as_str()
                                        .expect("query should be string")
                                        .to_string();
                                    (session_id, query)
                                };
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fuzzyFileSearch/sessionUpdated".to_string(),
                                            params: Some(serde_json::json!({
                                                "sessionId": session_id,
                                                "query": query,
                                                "files": [{
                                                    "root": "/tmp/reconnected-project",
                                                    "path": "docs/reconnected.md",
                                                    "match_type": "file",
                                                    "file_name": "reconnected.md",
                                                    "score": 100,
                                                    "indices": null,
                                                }],
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "fuzzyFileSearch/sessionStop" => {
                                let session_id =
                                    request
                                        .params
                                        .as_ref()
                                        .expect("fuzzyFileSearch/sessionStop params should exist")
                                        ["sessionId"]
                                        .as_str()
                                        .expect("session id should be string")
                                        .to_string();
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fuzzyFileSearch/sessionCompleted".to_string(),
                                            params: Some(serde_json::json!({
                                                "sessionId": session_id,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "windowsSandbox/setupStart" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "started": true,
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "windowsSandbox/setupCompleted".to_string(),
                                            params: Some(serde_json::json!({
                                                "mode": "unelevated",
                                                "success": true,
                                                "error": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "command/exec" => {
                                let process_id = request
                                    .params
                                    .as_ref()
                                    .expect("command/exec params should exist")["processId"]
                                    .as_str()
                                    .expect("process id should be string")
                                    .to_string();
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "command/exec/outputDelta".to_string(),
                                            params: Some(serde_json::json!({
                                                "processId": process_id,
                                                "stream": "stdout",
                                                "deltaBase64": "cmVtb3RlLWNvbW1hbmQ=",
                                                "capReached": false,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "exitCode": 0,
                                            "stdout": "",
                                            "stderr": "",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "mcpServer/oauth/login" => {
                                let name = request
                                    .params
                                    .as_ref()
                                    .expect("mcpServer/oauth/login params should exist")["name"]
                                    .as_str()
                                    .expect("mcp server name should be string")
                                    .to_string();
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "authorizationUrl": format!(
                                                "https://example.com/oauth/{name}"
                                            ),
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "mcpServer/oauthLogin/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "name": name,
                                                "success": true,
                                                "error": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "thread/read" => {
                                let thread_id = request
                                    .params
                                    .as_ref()
                                    .expect("thread/read params should exist")["id"]
                                    .as_str()
                                    .expect("thread id should be string");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "thread": {
                                                "id": thread_id,
                                                "status": "running",
                                                "title": "Recovered thread",
                                                "entries": [],
                                                "workingDirectory": null,
                                                "rootDir": null,
                                                "startedAt": null,
                                                "completedAt": null,
                                            }
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [{
                                                "id": "reconnected-thread",
                                                "status": "running",
                                                "title": "Recovered thread",
                                                "entries": [],
                                                "workingDirectory": null,
                                                "rootDir": null,
                                                "startedAt": null,
                                                "completedAt": null,
                                            }],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/branch" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "threadId": "reconnected-thread",
                                            "branchedThreadId": "reconnected-thread-branch",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/turns/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/turns/start" => {
                                let thread_id = request
                                    .params
                                    .as_ref()
                                    .expect("turn start params should exist")["threadId"]
                                    .as_str()
                                    .expect("thread id should be string");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "turnId": "reconnected-turn",
                                            "threadId": thread_id,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/turns/stop" => {
                                let thread_id = request
                                    .params
                                    .as_ref()
                                    .expect("turn stop params should exist")["threadId"]
                                    .as_str()
                                    .expect("thread id should be string");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": "reconnected-turn",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/realtime/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "data": [],
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/realtime/start" => {
                                let thread_id = request
                                    .params
                                    .as_ref()
                                    .expect("realtime start params should exist")["threadId"]
                                    .as_str()
                                    .expect("thread id should be string");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "threadId": thread_id,
                                            "realtimeId": "reconnected-realtime",
                                        }),
                                    }),
                                )
                                .await;
                            }
                            "thread/realtime/stop" => {
                                let thread_id = request
                                    .params
                                    .as_ref()
                                    .expect("realtime stop params should exist")["threadId"]
                                    .as_str()
                                    .expect("thread id should be string");
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({}),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "thread/realtime/closed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "reason": "client requested stop",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            method => panic!("unexpected request method: {method}"),
                        }
                    }
                }
                _ => unreachable!("unexpected reconnect connection index"),
            }
        }
    });
    format!("ws://{addr}")
}

fn reconnected_plugin_summary(plugin_installed: bool) -> serde_json::Value {
    serde_json::json!({
        "id": "reconnected-plugin",
        "name": "reconnected-plugin",
        "source": {
            "type": "local",
            "path": "/tmp/reconnected-project/plugins/reconnected-plugin",
        },
        "installed": plugin_installed,
        "enabled": plugin_installed,
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_INSTALL",
        "interface": {
            "displayName": "Recovered Plugin",
            "shortDescription": "Recovered plugin list",
            "longDescription": null,
            "developerName": null,
            "category": "tools",
            "capabilities": ["commands"],
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
    })
}

fn reconnected_plugin_list_response(plugin_installed: bool) -> serde_json::Value {
    serde_json::json!({
        "marketplaces": [{
            "name": "reconnected-marketplace",
            "path": "/tmp/reconnected-project/marketplace.json",
            "interface": {
                "displayName": "Recovered Marketplace",
            },
            "plugins": [reconnected_plugin_summary(plugin_installed)],
        }],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": ["reconnected-plugin"],
    })
}

fn reconnected_plugin_read_response(plugin_installed: bool) -> serde_json::Value {
    serde_json::json!({
        "plugin": {
            "marketplaceName": "reconnected-marketplace",
            "marketplacePath": "/tmp/reconnected-project/marketplace.json",
            "summary": reconnected_plugin_summary(plugin_installed),
            "description": "Recovered plugin detail",
            "skills": [{
                "name": "reconnected-skill",
                "description": "Recovered single-worker skill",
                "shortDescription": "Recovered skill",
                "interface": null,
                "path": "/tmp/reconnected-project/skills/reconnected-skill",
                "enabled": true,
            }],
            "hooks": [],
            "appTemplates": [],
            "apps": [],
            "mcpServers": ["reconnected-mcp"],
        },
    })
}
