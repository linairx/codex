use super::*;

pub(crate) async fn start_mock_remote_workflow_server() -> String {
    start_mock_remote_workflow_server_with_thread_id("thread-remote-workflow").await
}

pub(crate) async fn start_mock_remote_workflow_server_with_thread_id(
    thread_id: &'static str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");

                expect_remote_initialize(&mut websocket).await;

                let review_thread_id = "thread-review-remote-workflow";
                let preview = "/tmp/remote-project";
                let review_preview = "/tmp/remote-project/review";
                let mut thread_name: Option<String> = None;
                let mut plugin_installed = false;
                let mut next_login_id = 0usize;
                let mut pending_login_id: Option<String> = None;
                let mut account = serde_json::Value::Null;
                let turn_id = "turn-remote-workflow";
                let review_turn_id = "turn-review-remote-workflow";

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "account/read" => serde_json::json!({
                            "account": account,
                            "requiresOpenaiAuth": false,
                        }),
                        "getAuthStatus" => serde_json::json!({
                            "authMethod": null,
                            "authToken": "remote-auth-token",
                            "requiresOpenaiAuth": false,
                        }),
                        "account/rateLimits/read" => serde_json::json!({
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
                        "model/list" => serde_json::json!({
                            "data": [{
                                "id": "openai/gpt-5",
                                "model": "gpt-5",
                                "upgrade": null,
                                "upgradeInfo": null,
                                "availabilityNux": null,
                                "displayName": "GPT-5",
                                "description": "Remote gateway workflow model",
                                "hidden": false,
                                "supportedReasoningEfforts": [{
                                    "reasoningEffort": "medium",
                                    "description": "Balanced"
                                }],
                                "defaultReasoningEffort": "medium",
                                "inputModalities": ["text"],
                                "supportsPersonality": false,
                                "additionalSpeedTiers": [],
                                "isDefault": true
                            }],
                            "nextCursor": null,
                        }),
                        "externalAgentConfig/detect" => serde_json::json!({
                            "items": [],
                        }),
                        "externalAgentConfig/import" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({
                                        "importId": "import-1",
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "externalAgentConfig/import/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "importId": "import-1",
                                            "itemTypeResults": [],
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "config/read" => serde_json::json!({
                            "config": {
                                "model": "gpt-5",
                            },
                            "origins": {},
                            "layers": null,
                        }),
                        "configRequirements/read" => serde_json::json!({
                            "requirements": null,
                        }),
                        "experimentalFeature/list" => serde_json::json!({
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
                        "experimentalFeature/enablement/set" => {
                            let enablement = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("enablement"))
                                .cloned()
                                .expect(
                                    "experimentalFeature/enablement/set should include enablement",
                                );
                            serde_json::json!({
                                "enablement": enablement,
                            })
                        }
                        "collaborationMode/list" => serde_json::json!({
                            "data": [{
                                "name": "default",
                                "mode": "default",
                                "model": null,
                                "reasoningEffort": null,
                            }],
                        }),
                        "account/login/start" => {
                            if request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("type"))
                                .and_then(serde_json::Value::as_str)
                                == Some("chatgptAuthTokens")
                            {
                                account = serde_json::json!({
                                    "type": "chatgpt",
                                    "email": "remote@example.com",
                                    "planType": "pro",
                                });
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "type": "chatgptAuthTokens",
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "account/login/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "loginId": null,
                                                "success": true,
                                                "error": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "account/updated".to_string(),
                                            params: Some(serde_json::json!({
                                                "authMode": "chatgptAuthTokens",
                                                "planType": "pro",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                continue;
                            }
                            next_login_id += 1;
                            let login_id = format!("remote-login-{next_login_id}");
                            if next_login_id == 1 {
                                pending_login_id = Some(login_id.clone());
                                serde_json::json!({
                                    "type": "chatgptDeviceCode",
                                    "loginId": login_id,
                                    "verificationUrl": "https://example.com/device",
                                    "userCode": "REMOTE-CODE",
                                })
                            } else {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "type": "chatgpt",
                                            "loginId": login_id,
                                            "authUrl": "https://example.com/login",
                                        }),
                                    }),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "account/login/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "loginId": login_id,
                                                "success": true,
                                                "error": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                continue;
                            }
                        }
                        "account/login/cancel" => {
                            let login_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("loginId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("account/login/cancel should include loginId");
                            if pending_login_id.as_deref() == Some(login_id) {
                                pending_login_id = None;
                                serde_json::json!({
                                    "status": "canceled",
                                })
                            } else {
                                serde_json::json!({
                                    "status": "notFound",
                                })
                            }
                        }
                        "app/list" => serde_json::json!({
                            "data": [{
                                "id": "remote-app",
                                "name": "Remote App",
                                "description": "Remote app description",
                                "logoUrl": null,
                                "logoUrlDark": null,
                                "distributionChannel": null,
                                "branding": null,
                                "appMetadata": null,
                                "labels": null,
                                "installUrl": null,
                                "isAccessible": false,
                                "isEnabled": true,
                                "pluginDisplayNames": [],
                            }],
                            "nextCursor": null,
                        }),
                        "skills/list" => serde_json::json!({
                            "data": [{
                                "cwd": preview,
                                "skills": [],
                                "errors": [],
                            }],
                        }),
                        "skills/config/write" => {
                            pretty_assertions::assert_eq!(
                                request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("enabled"))
                                    .and_then(serde_json::Value::as_bool),
                                Some(true)
                            );
                            serde_json::json!({
                                "effectiveEnabled": true,
                            })
                        }
                        "mcpServerStatus/list" => serde_json::json!({
                            "data": [{
                                "name": "remote-mcp",
                                "tools": {},
                                "resources": [],
                                "resourceTemplates": [],
                                "authStatus": "bearerToken",
                            }],
                            "nextCursor": null,
                        }),
                        "config/mcpServer/reload" => serde_json::json!({}),
                        "mcpServer/oauth/login" => {
                            let name = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("name"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/oauth/login should include name");
                            pretty_assertions::assert_eq!(name, "remote-mcp");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({
                                        "authorizationUrl": "https://example.com/oauth/remote-mcp",
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
                                            "name": "remote-mcp",
                                            "success": true,
                                            "error": null,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "mcpServer/resource/read" => {
                            let uri = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("uri"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/resource/read should include uri");
                            serde_json::json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "text/markdown",
                                    "text": "remote resource",
                                }],
                            })
                        }
                        "mcpServer/tool/call" => {
                            let tool = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("tool"))
                                .and_then(serde_json::Value::as_str)
                                .expect("mcpServer/tool/call should include tool");
                            pretty_assertions::assert_eq!(tool, "lookup");
                            serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": "remote tool result",
                                }],
                                "structuredContent": {
                                    "matches": 1,
                                },
                                "isError": false,
                            })
                        }
                        "plugin/list" => serde_json::json!({
                            "marketplaces": [{
                                "name": "remote-marketplace",
                                "path": format!("{preview}/marketplace.json"),
                                "interface": {
                                    "displayName": "Remote Marketplace",
                                },
                                "plugins": [{
                                    "id": "remote-plugin",
                                    "name": "remote-plugin",
                                    "source": {
                                        "type": "local",
                                        "path": format!("{preview}/plugins/remote-plugin"),
                                    },
                                    "installed": plugin_installed,
                                    "enabled": plugin_installed,
                                    "installPolicy": "AVAILABLE",
                                    "authPolicy": "ON_INSTALL",
                                    "interface": {
                                        "displayName": "Remote Plugin",
                                        "shortDescription": "Remote plugin short description",
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
                                }],
                            }],
                            "marketplaceLoadErrors": [],
                            "featuredPluginIds": ["remote-plugin"],
                        }),
                        "marketplace/add" => serde_json::json!({
                            "marketplaceName": "remote-marketplace",
                            "installedRoot": format!("{preview}/marketplace"),
                            "alreadyAdded": false,
                        }),
                        "plugin/read" => serde_json::json!({
                            "plugin": {
                                "marketplaceName": "remote-marketplace",
                                "marketplacePath": format!("{preview}/marketplace.json"),
                                "summary": {
                                    "id": "remote-plugin",
                                    "name": "remote-plugin",
                                    "source": {
                                        "type": "local",
                                        "path": format!("{preview}/plugins/remote-plugin"),
                                    },
                                    "installed": plugin_installed,
                                    "enabled": plugin_installed,
                                    "installPolicy": "AVAILABLE",
                                    "authPolicy": "ON_INSTALL",
                                    "interface": {
                                        "displayName": "Remote Plugin",
                                        "shortDescription": "Remote plugin short description",
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
                                },
                                "description": "Remote plugin detail",
                                "skills": [{
                                    "name": "remote-skill",
                                    "description": "Remote skill description",
                                    "shortDescription": "Remote skill short description",
                                    "interface": null,
                                    "path": format!("{preview}/skills/remote-skill"),
                                    "enabled": true,
                                }],
                                "hooks": [],
                                "appTemplates": [],
                                "apps": [],
                                "mcpServers": ["remote-mcp"],
                            },
                        }),
                        "plugin/install" => {
                            plugin_installed = true;
                            serde_json::json!({
                                "authPolicy": "ON_INSTALL",
                                "appsNeedingAuth": [],
                            })
                        }
                        "plugin/uninstall" => {
                            plugin_installed = false;
                            serde_json::json!({})
                        }
                        "config/value/write" => serde_json::json!({
                            "status": "ok",
                            "version": "remote-version-1",
                            "filePath": format!("{preview}/config.toml"),
                            "overriddenMetadata": null,
                        }),
                        "config/batchWrite" => serde_json::json!({
                            "status": "ok",
                            "version": "remote-version-1",
                            "filePath": format!("{preview}/config.toml"),
                            "overriddenMetadata": null,
                        }),
                        "fs/readFile" => serde_json::json!({
                            "dataBase64": "cmVtb3RlLWZpbGU=",
                        }),
                        "fs/writeFile" => serde_json::json!({}),
                        "fs/createDirectory" => serde_json::json!({}),
                        "fs/getMetadata" => serde_json::json!({
                            "isDirectory": false,
                            "isFile": true,
                            "isSymlink": false,
                            "createdAtMs": 0,
                            "modifiedAtMs": 0,
                        }),
                        "fs/readDirectory" => serde_json::json!({
                            "entries": [{
                                "fileName": "gateway.txt",
                                "isDirectory": false,
                                "isFile": true,
                            }],
                        }),
                        "fs/remove" | "fs/copy" => serde_json::json!({}),
                        "fs/watch" => {
                            let watch_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("watchId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("fs/watch should include watchId");
                            pretty_assertions::assert_eq!(watch_id, "remote-watch");
                            let path = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("path"))
                                .cloned()
                                .expect("fs/watch should include path");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "fs/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "watchId": watch_id,
                                            "changedPaths": [path.clone()],
                                        })),
                                    },
                                ),
                            )
                            .await;
                            serde_json::json!({
                                "path": path,
                            })
                        }
                        "fs/unwatch" => serde_json::json!({}),
                        "fuzzyFileSearch" => serde_json::json!({
                            "files": [{
                                "root": "/tmp/remote-project",
                                "path": "docs/gateway.md",
                                "match_type": "file",
                                "file_name": "gateway.md",
                                "score": 42,
                                "indices": [5, 6, 7, 8],
                            }],
                        }),
                        "gitDiffToRemote" => serde_json::json!({
                            "sha": "0123456789abcdef0123456789abcdef01234567",
                            "diff": "diff --git a/docs/gateway.md b/docs/gateway.md\n",
                        }),
                        "fuzzyFileSearch/sessionStart" => serde_json::json!({}),
                        "fuzzyFileSearch/sessionUpdate" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "fuzzyFileSearch/sessionUpdated".to_string(),
                                        params: Some(serde_json::json!({
                                            "sessionId": "remote-fuzzy-session",
                                            "query": "gate",
                                            "files": [{
                                                "root": "/tmp/remote-project",
                                                "path": "docs/gateway.md",
                                                "match_type": "file",
                                                "file_name": "gateway.md",
                                                "score": 42,
                                                "indices": [5, 6, 7, 8],
                                            }],
                                        })),
                                    },
                                ),
                            )
                            .await;
                            serde_json::json!({})
                        }
                        "fuzzyFileSearch/sessionStop" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "fuzzyFileSearch/sessionCompleted".to_string(),
                                        params: Some(serde_json::json!({
                                            "sessionId": "remote-fuzzy-session",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            serde_json::json!({})
                        }
                        "windowsSandbox/setupStart" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
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
                            continue;
                        }
                        "command/exec" => {
                            let process_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("processId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("command/exec should include processId");
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
                            serde_json::json!({
                                "exitCode": 0,
                                "stdout": "",
                                "stderr": "",
                            })
                        }
                        "command/exec/write" => {
                            let process_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("processId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("command/exec/write should include processId");
                            pretty_assertions::assert_eq!(process_id, "proc-remote");
                            serde_json::json!({})
                        }
                        "command/exec/resize" => {
                            let process_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("processId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("command/exec/resize should include processId");
                            pretty_assertions::assert_eq!(process_id, "proc-remote");
                            serde_json::json!({})
                        }
                        "command/exec/terminate" => {
                            let process_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("processId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("command/exec/terminate should include processId");
                            pretty_assertions::assert_eq!(process_id, "proc-remote");
                            serde_json::json!({})
                        }
                        "feedback/upload" => serde_json::json!({
                            "threadId": "feedback-thread-remote",
                        }),
                        "account/sendAddCreditsNudgeEmail" => serde_json::json!({
                            "status": "sent",
                        }),
                        "memory/reset" | "account/logout" => serde_json::json!({}),
                        "thread/start" => {
                            write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                            "model": "gpt-5",
                                            "modelProvider": "openai",
                                            "serviceTier": null,
                                            "cwd": preview,
                                            "instructionSources": [],
                                            "approvalPolicy": "never",
                                            "approvalsReviewer": "user",
                                            "sandbox": {
                                                "type": "dangerFullAccess"
                                            },
                                            "reasoningEffort": null,
                                        }),
                                    }),
                                )
                                .await;
                            write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                        })),
                                    }),
                                )
                                .await;
                            continue;
                        }
                        "thread/list" => serde_json::json!({
                            "data": [mock_thread_with_name(thread_id, preview, thread_name.as_deref())],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        "thread/loaded/list" => serde_json::json!({
                            "data": [thread_id],
                            "nextCursor": null,
                        }),
                        "thread/read" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            if requested_thread_id == review_thread_id {
                                serde_json::json!({
                                    "thread": mock_thread(review_thread_id, review_preview),
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": review_preview,
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess"
                                    },
                                    "reasoningEffort": null,
                                })
                            } else {
                                serde_json::json!({
                                    "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                                    "model": "gpt-5",
                                    "modelProvider": "openai",
                                    "serviceTier": null,
                                    "cwd": preview,
                                    "instructionSources": [],
                                    "approvalPolicy": "never",
                                    "approvalsReviewer": "user",
                                    "sandbox": {
                                        "type": "dangerFullAccess"
                                    },
                                    "reasoningEffort": null,
                                })
                            }
                        }
                        "thread/name/set" => {
                            let name = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("name"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/name/set should include name");
                            thread_name = Some(name.to_string());
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/name/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "threadName": name,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/memoryMode/set" => serde_json::json!({}),
                        "thread/unsubscribe" => serde_json::json!({
                            "status": "unsubscribed",
                        }),
                        "thread/archive" => serde_json::json!({}),
                        "thread/unarchive" => serde_json::json!({
                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                        }),
                        "thread/metadata/update" => serde_json::json!({
                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                        }),
                        "thread/turns/list" => serde_json::json!({
                            "data": [mock_turn(turn_id, "completed")],
                            "nextCursor": null,
                            "backwardsCursor": null,
                        }),
                        "thread/increment_elicitation" => serde_json::json!({
                            "count": 1,
                            "paused": true,
                        }),
                        "thread/decrement_elicitation" => serde_json::json!({
                            "count": 0,
                            "paused": false,
                        }),
                        "thread/inject_items" => serde_json::json!({}),
                        "thread/compact/start" => serde_json::json!({}),
                        "thread/shellCommand" => serde_json::json!({}),
                        "thread/backgroundTerminals/clean" => serde_json::json!({}),
                        "thread/rollback" => serde_json::json!({
                            "thread": mock_thread_with_name(thread_id, preview, thread_name.as_deref()),
                        }),
                        "thread/realtime/listVoices" => serde_json::json!({
                            "voices": serde_json::to_value(RealtimeVoicesList::builtin())
                                .expect("builtin voices should serialize"),
                        }),
                        "thread/realtime/start" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "realtimeSessionId": "session-remote-workflow",
                                            "version": "v2",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/itemAdded".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "item": {
                                                "type": "message",
                                                "id": "item-remote-workflow",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/realtime/appendText" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/transcript/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "role": "assistant",
                                            "delta": "remote realtime delta",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/transcript/done".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "role": "assistant",
                                            "text": "remote realtime done",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/realtime/appendAudio" => {
                            let audio = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("audio"))
                                .cloned()
                                .expect("realtime appendAudio should include audio");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({}),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/outputAudio/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "audio": audio,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/sdp".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "sdp": "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=Codex\r\n",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/realtime/error".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "message": "remote realtime warning",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/realtime/stop" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
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
                                            "reason": "remote-client-requested",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "review/start" => serde_json::json!({
                            "turn": mock_turn(review_turn_id, "inProgress"),
                            "reviewThreadId": review_thread_id,
                        }),
                        "turn/start" => {
                            let is_plan_mode = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("collaborationMode"))
                                .and_then(|collaboration_mode| collaboration_mode.get("mode"))
                                .and_then(serde_json::Value::as_str)
                                == Some("plan");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id.clone(),
                                    result: serde_json::json!({
                                        "turn": mock_turn(turn_id, "inProgress"),
                                    }),
                                }),
                            )
                            .await;
                            if is_plan_mode {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/started".to_string(),
                                            params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "startedAtMs": 0,
                                            "item": {
                                                "type": "plan",
                                                "id": "plan-remote-workflow",
                                                "text": "",
                                            },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "item/completed".to_string(),
                                            params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "completedAtMs": 0,
                                            "item": {
                                                "type": "plan",
                                                "id": "plan-remote-workflow",
                                                "text": "- Remote step\n",
                                            },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "status": {
                                                "type": "active",
                                                "activeFlags": [],
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryTextDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "reasoning-summary-remote-workflow",
                                            "delta": "remote summary",
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "reasoning-remote-workflow",
                                            "delta": "remote reasoning",
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "exec-remote-workflow",
                                            "delta": "remote stdout",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "patch-remote-workflow",
                                            "delta": "remote patch",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/plan/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "plan-remote-workflow",
                                            "delta": "remote plan",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryPartAdded".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "reasoning-summary-remote-workflow",
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/terminalInteraction"
                                            .to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "terminal-remote-workflow",
                                            "processId": "proc-remote-workflow",
                                            "stdin": "y\n",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/diff/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "diff": "remote diff",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/plan/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "explanation": "single-worker remote plan",
                                            "plan": [{
                                                "step": "remote plan step",
                                                "status": "completed",
                                            }],
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/tokenUsage/updated".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "tokenUsage": {
                                                "total": {
                                                    "totalTokens": 42,
                                                    "inputTokens": 20,
                                                    "cachedInputTokens": 3,
                                                    "outputTokens": 22,
                                                    "reasoningOutputTokens": 7,
                                                },
                                                "last": {
                                                    "totalTokens": 10,
                                                    "inputTokens": 4,
                                                    "cachedInputTokens": 1,
                                                    "outputTokens": 6,
                                                    "reasoningOutputTokens": 2,
                                                },
                                                "modelContextWindow": 200000,
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/mcpToolCall/progress".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "mcp-remote-workflow",
                                            "message": "remote mcp progress",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/compacted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "model/rerouted".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "fromModel": "gpt-5",
                                            "toModel": "gpt-5-codex",
                                            "reason": "highRiskCyberActivity",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "rawResponseItem/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "completedAtMs": 0,
                                            "item": {
                                                "type": "other",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/autoApprovalReview/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "startedAtMs": 0,
                                            "reviewId": format!("guardian-{thread_id}"),
                                            "targetItemId": format!("cmd-{thread_id}"),
                                            "review": {
                                                "status": "inProgress",
                                                "riskLevel": null,
                                                "userAuthorization": null,
                                                "rationale": null,
                                            },
                                            "action": {
                                                "type": "command",
                                                "source": "shell",
                                                "command": "cat docs/gateway.md",
                                                "cwd": "/tmp",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/autoApprovalReview/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "startedAtMs": 0,
                                            "completedAtMs": 0,
                                            "reviewId": format!("guardian-{thread_id}"),
                                            "targetItemId": format!("cmd-{thread_id}"),
                                            "decisionSource": "agent",
                                            "review": {
                                                "status": "approved",
                                                "riskLevel": "low",
                                                "userAuthorization": "low",
                                                "rationale": "Read-only command.",
                                            },
                                            "action": {
                                                "type": "command",
                                                "source": "shell",
                                                "command": "cat docs/gateway.md",
                                                "cwd": "/tmp",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "error".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "willRetry": false,
                                            "error": {
                                                "message": "remote workflow warning",
                                                "codexErrorInfo": null,
                                                "additionalDetails": "single-worker remote warning",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(turn_id, "inProgress"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "hook/started".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "run": {
                                                    "id": format!("hook-{thread_id}"),
                                                    "eventName": "userPromptSubmit",
                                                    "handlerType": "command",
                                                    "executionMode": "sync",
                                                    "scope": "turn",
                                                    "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                                                    "source": "user",
                                                    "displayOrder": 0,
                                                    "status": "running",
                                                    "statusMessage": format!("hook started {thread_id}"),
                                                    "startedAt": 1,
                                                    "completedAt": null,
                                                    "durationMs": null,
                                                    "entries": [],
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "startedAtMs": 0,
                                            "item": {
                                                "type": "agentMessage",
                                                "id": format!("msg-{thread_id}"),
                                                "text": "streaming answer in progress",
                                                "phase": "commentary",
                                                "memoryCitation": null,
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/agentMessage/delta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": "msg-remote-workflow",
                                            "delta": "hello back",
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryTextDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-summary-{thread_id}"),
                                            "delta": format!("summary {thread_id}"),
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-{thread_id}"),
                                            "delta": format!("reasoning {thread_id}"),
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("exec-{thread_id}"),
                                            "delta": format!("stdout {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("patch-{thread_id}"),
                                            "delta": format!("patch {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryTextDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-summary-{thread_id}"),
                                            "delta": format!("summary {thread_id}"),
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-{thread_id}"),
                                            "delta": format!("reasoning {thread_id}"),
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("exec-{thread_id}"),
                                            "delta": format!("stdout {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("patch-{thread_id}"),
                                            "delta": format!("patch {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/summaryTextDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-summary-{thread_id}"),
                                            "delta": format!("summary {thread_id}"),
                                            "summaryIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/reasoning/textDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("reasoning-{thread_id}"),
                                            "delta": format!("reasoning {thread_id}"),
                                            "contentIndex": 0,
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/commandExecution/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("exec-{thread_id}"),
                                            "delta": format!("stdout {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/fileChange/outputDelta".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "itemId": format!("patch-{thread_id}"),
                                            "delta": format!("patch {thread_id}"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "hook/completed".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": thread_id,
                                                "turnId": turn_id,
                                                "run": {
                                                    "id": format!("hook-{thread_id}"),
                                                    "eventName": "userPromptSubmit",
                                                    "handlerType": "command",
                                                    "executionMode": "sync",
                                                    "scope": "turn",
                                                    "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                                                    "source": "user",
                                                    "displayOrder": 0,
                                                    "status": "completed",
                                                    "statusMessage": format!("hook completed {thread_id}"),
                                                    "startedAt": 1,
                                                    "completedAt": 2,
                                                    "durationMs": 1,
                                                    "entries": [],
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "item/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turnId": turn_id,
                                            "completedAtMs": 0,
                                            "item": {
                                                "type": "agentMessage",
                                                "id": format!("msg-{thread_id}"),
                                                "text": "streaming answer completed",
                                                "phase": "final_answer",
                                                "memoryCitation": null,
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "turn/completed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "turn": mock_turn(turn_id, "completed"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": thread_id,
                                            "status": {
                                                "type": "idle",
                                            },
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/resume"
                            if request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("history"))
                                .is_some_and(|history| !history.is_null()) =>
                        {
                            serde_json::json!({
                                "thread": mock_thread(thread_id, preview),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": preview,
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/resume" | "thread/fork" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Error(JSONRPCError {
                                    id: request.id,
                                    error: JSONRPCErrorError {
                                        code: -32600,
                                        message: format!(
                                            "no rollout found for thread id {thread_id}"
                                        ),
                                        data: None,
                                    },
                                }),
                            )
                            .await;
                            continue;
                        }
                        method => panic!("unexpected request method: {method}"),
                    };
                    write_websocket_message(
                        &mut websocket,
                        JSONRPCMessage::Response(JSONRPCResponse {
                            id: request.id,
                            result,
                        }),
                    )
                    .await;
                }
            });
        }
    });
    format!("ws://{addr}")
}
