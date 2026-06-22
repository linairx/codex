use super::*;

pub(crate) async fn start_reconnecting_v2_multi_connection_bootstrap_setup_server(
    config: MultiConnectionBootstrapSetupConfig,
) -> String {
    let MultiConnectionBootstrapSetupConfig {
        worker_label,
        requires_openai_auth,
        rate_limits,
        models,
        apps,
        mcp_status_names,
        shared_cwd,
        unique_cwd,
    } = config;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        for connection_index in 0..4 {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let rate_limits = rate_limits.clone();
            let models = models.clone();
            let apps = apps.clone();
            let mcp_status_names = mcp_status_names.clone();
            let realtime_voices = bootstrap_setup_realtime_voices(worker_label);
            let mut websocket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket upgrade should succeed");
            expect_remote_initialize(&mut websocket).await;

            match connection_index {
                0 | 2 => {
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
                3 => {
                    let mut requires_openai_auth = requires_openai_auth;
                    let mut next_login_id = 0usize;
                    let mut pending_login_id: Option<String> = None;
                    let mut account = serde_json::Value::Null;

                    loop {
                        let request = read_websocket_request(&mut websocket).await;
                        let method = request.method.clone();
                        let result = match request.method.as_str() {
                            "account/read" => serde_json::json!({
                                "account": account,
                                "requiresOpenaiAuth": requires_openai_auth,
                            }),
                            "getAuthStatus" => {
                                let auth_token = if worker_label == "worker-a" {
                                    serde_json::json!("worker-a-token")
                                } else {
                                    serde_json::Value::Null
                                };
                                serde_json::json!({
                                    "authMethod": null,
                                    "authToken": auth_token,
                                    "requiresOpenaiAuth": requires_openai_auth,
                                })
                            }
                            "account/rateLimits/read" => {
                                let primary = rate_limits
                                    .first()
                                    .expect("rate limits should include a primary snapshot");
                                let rate_limits_by_limit_id = rate_limits
                                    .iter()
                                    .map(|(limit_id, limit_name, used_percent)| {
                                        (
                                            (*limit_id).to_string(),
                                            serde_json::json!({
                                                "limitId": limit_id,
                                                "limitName": limit_name,
                                                "primary": {
                                                    "usedPercent": used_percent,
                                                    "windowMinutes": 300,
                                                    "resetsAt": 1_700_000_000,
                                                },
                                                "secondary": null,
                                                "credits": null,
                                                "planType": null,
                                                "rateLimitReachedType": null,
                                            }),
                                        )
                                    })
                                    .collect::<serde_json::Map<String, serde_json::Value>>();
                                serde_json::json!({
                                    "rateLimits": {
                                        "limitId": primary.0,
                                        "limitName": primary.1,
                                        "primary": {
                                            "usedPercent": primary.2,
                                            "windowMinutes": 300,
                                            "resetsAt": 1_700_000_000,
                                        },
                                        "secondary": null,
                                        "credits": null,
                                        "planType": null,
                                        "rateLimitReachedType": null,
                                    },
                                    "rateLimitsByLimitId": rate_limits_by_limit_id,
                                })
                            }
                            "model/list" => serde_json::json!({
                                "data": models.iter().map(|(id, display_name, is_default)| serde_json::json!({
                                    "id": id,
                                    "model": id,
                                    "upgrade": null,
                                    "upgradeInfo": null,
                                    "availabilityNux": null,
                                    "displayName": display_name,
                                    "description": format!("{display_name} description"),
                                    "hidden": false,
                                    "supportedReasoningEfforts": [{
                                        "reasoningEffort": "medium",
                                        "description": "Balanced"
                                    }],
                                    "defaultReasoningEffort": "medium",
                                    "inputModalities": ["text"],
                                    "supportsPersonality": false,
                                    "additionalSpeedTiers": [],
                                    "isDefault": is_default,
                                })).collect::<Vec<_>>(),
                                "nextCursor": null,
                            }),
                            "externalAgentConfig/detect" => serde_json::json!({
                                "items": [
                                    {
                                        "itemType": "PLUGINS",
                                        "description": "shared config",
                                        "cwd": shared_cwd,
                                        "details": null,
                                    },
                                    {
                                        "itemType": "PLUGINS",
                                        "description": format!("{worker_label} repo config"),
                                        "cwd": unique_cwd,
                                        "details": null,
                                    },
                                ],
                            }),
                            "config/read" => {
                                let requested_cwd = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("cwd"))
                                    .and_then(serde_json::Value::as_str);
                                let config_cwd = if requested_cwd
                                    .is_some_and(|cwd| cwd.starts_with(unique_cwd))
                                {
                                    unique_cwd
                                } else {
                                    shared_cwd
                                };
                                let layer_name = if config_cwd == unique_cwd {
                                    serde_json::json!({
                                        "type": "project",
                                        "dotCodexFolder": config_cwd,
                                    })
                                } else {
                                    serde_json::json!({
                                        "type": "user",
                                        "file": format!("{config_cwd}/config.toml"),
                                    })
                                };
                                serde_json::json!({
                                    "config": {
                                        "model": format!("gpt-5-{worker_label}"),
                                    },
                                    "origins": {},
                                    "layers": [{
                                        "name": layer_name,
                                        "version": format!("{worker_label}-config-version"),
                                        "config": {
                                            "model": format!("gpt-5-{worker_label}"),
                                        },
                                        "disabledReason": null,
                                    }],
                                })
                            }
                            "configRequirements/read" => serde_json::json!({
                                "requirements": null,
                            }),
                            "experimentalFeature/list" => serde_json::json!({
                                "data": [{
                                    "name": format!("gateway-test-feature-{worker_label}"),
                                    "stage": "beta",
                                    "displayName": format!("Gateway Test Feature {worker_label}"),
                                    "description": format!(
                                        "Used by gateway multi-worker passthrough tests for {worker_label}"
                                    ),
                                    "announcement": null,
                                    "enabled": false,
                                    "defaultEnabled": false,
                                }],
                                "nextCursor": null,
                            }),
                            "collaborationMode/list" => serde_json::json!({
                                "data": [{
                                    "name": format!("{worker_label}-default"),
                                    "mode": "default",
                                    "model": format!("gpt-5-{worker_label}"),
                                    "reasoningEffort": null,
                                }],
                            }),
                            "account/login/start" => {
                                match request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("type"))
                                    .and_then(serde_json::Value::as_str)
                                {
                                    Some("apiKey") => {
                                        requires_openai_auth = false;
                                        account = serde_json::json!({
                                            "type": "apiKey",
                                        });
                                        write_websocket_message(
                                            &mut websocket,
                                            JSONRPCMessage::Response(JSONRPCResponse {
                                                id: request.id.clone(),
                                                result: serde_json::json!({
                                                    "type": "apiKey",
                                                }),
                                            }),
                                        )
                                        .await;
                                        write_websocket_message(
                                            &mut websocket,
                                            JSONRPCMessage::Notification(
                                                codex_app_server_protocol::JSONRPCNotification {
                                                    method: "account/updated".to_string(),
                                                    params: Some(serde_json::json!({
                                                        "authMode": "apikey",
                                                        "planType": null,
                                                    })),
                                                },
                                            ),
                                        )
                                        .await;
                                        continue;
                                    }
                                    Some("chatgptAuthTokens") => {
                                        requires_openai_auth = false;
                                        account = serde_json::json!({
                                            "type": "chatgpt",
                                            "email": format!("{worker_label}@example.com"),
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
                                    _ => {}
                                }
                                next_login_id += 1;
                                let login_id = format!("{worker_label}-login-{next_login_id}");
                                if next_login_id == 1 {
                                    pending_login_id = Some(login_id.clone());
                                    serde_json::json!({
                                        "type": "chatgptDeviceCode",
                                        "loginId": login_id,
                                        "verificationUrl": "https://example.com/device",
                                        "userCode": format!("{worker_label}-CODE"),
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
                                "data": apps.iter().map(|(id, name)| serde_json::json!({
                                    "id": id,
                                    "name": name,
                                    "description": format!("{name} description"),
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
                                })).collect::<Vec<_>>(),
                                "nextCursor": null,
                            }),
                            "skills/list" => serde_json::json!({
                                "data": [{
                                    "cwd": shared_cwd,
                                    "skills": [
                                        {
                                            "name": "shared-skill",
                                            "description": "Shared skill",
                                            "path": format!("{shared_cwd}/shared"),
                                            "scope": "repo",
                                            "enabled": true,
                                        },
                                        {
                                            "name": format!("{worker_label}-skill"),
                                            "description": format!("Skill from {worker_label}"),
                                            "path": format!("{unique_cwd}/skill"),
                                            "scope": "repo",
                                            "enabled": true,
                                        },
                                    ],
                                    "errors": [
                                        {
                                            "path": format!("{shared_cwd}/SKILL.md"),
                                            "message": "shared warning",
                                        },
                                        {
                                            "path": format!("{unique_cwd}/SKILL.md"),
                                            "message": format!("{worker_label} warning"),
                                        },
                                    ],
                                }],
                            }),
                            "plugin/list" => {
                                bootstrap_setup_plugin_list_json(worker_label, unique_cwd)
                            }
                            "mcpServerStatus/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id.clone(),
                                        result: serde_json::json!({
                                            "data": mcp_status_names.iter().map(|name| serde_json::json!({
                                                "name": name,
                                                "tools": {},
                                                "resources": [],
                                                "resourceTemplates": [],
                                                "authStatus": "bearerToken",
                                            })).collect::<Vec<_>>(),
                                            "nextCursor": null,
                                        }),
                                    }),
                                )
                                .await;
                                if let Some(worker_mcp_name) = mcp_status_names
                                    .iter()
                                    .find(|name| name.starts_with(worker_label))
                                {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Notification(
                                            codex_app_server_protocol::JSONRPCNotification {
                                                method: "mcpServer/startupStatus/updated"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "name": worker_mcp_name,
                                                    "status": "ready",
                                                    "error": null,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                }
                                continue;
                            }
                            "mcpServer/oauth/login" => {
                                let name = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("name"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("mcpServer/oauth/login should include name");
                                if mcp_status_names.contains(&name) {
                                    write_websocket_message(
                                        &mut websocket,
                                        JSONRPCMessage::Response(JSONRPCResponse {
                                            id: request.id.clone(),
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
                                                method: "mcpServer/oauthLogin/completed"
                                                    .to_string(),
                                                params: Some(serde_json::json!({
                                                    "name": name,
                                                    "success": true,
                                                    "error": null,
                                                })),
                                            },
                                        ),
                                    )
                                    .await;
                                    continue;
                                }
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Error(JSONRPCError {
                                        id: request.id,
                                        error: JSONRPCErrorError {
                                            code: -32602,
                                            message: format!(
                                                "mcpServer/oauth/login missing on {worker_label}"
                                            ),
                                            data: None,
                                        },
                                    }),
                                )
                                .await;
                                continue;
                            }
                            "thread/realtime/listVoices" => serde_json::json!({
                                "voices": serde_json::to_value(&realtime_voices)
                                    .expect("realtime voices should serialize"),
                            }),
                            "feedback/upload" => serde_json::json!({
                                "threadId": format!("feedback-thread-{worker_label}"),
                            }),
                            "account/sendAddCreditsNudgeEmail" => serde_json::json!({
                                "status": "sent",
                            }),
                            "fs/createDirectory" => serde_json::json!({}),
                            "fs/writeFile" => serde_json::json!({}),
                            "fs/readFile" => serde_json::json!({
                                "dataBase64": match worker_label {
                                    "worker-a" => "d29ya2VyLWEtZmlsZQ==",
                                    "worker-b" => "d29ya2VyLWItZmlsZQ==",
                                    other => panic!("unexpected worker label: {other}"),
                                },
                            }),
                            "fs/getMetadata" => serde_json::json!({
                                "isDirectory": false,
                                "isFile": true,
                                "isSymlink": false,
                                "createdAtMs": 0,
                                "modifiedAtMs": 0,
                            }),
                            "fs/readDirectory" => serde_json::json!({
                                "entries": [{
                                    "fileName": "worker-a-gateway.txt",
                                    "isDirectory": false,
                                    "isFile": true,
                                }],
                            }),
                            "fs/copy" => serde_json::json!({}),
                            "fs/remove" => serde_json::json!({}),
                            "fuzzyFileSearch" => {
                                multi_worker_fuzzy_file_search_response(worker_label)
                            }
                            "fuzzyFileSearch/sessionStart" => serde_json::json!({}),
                            "fuzzyFileSearch/sessionUpdate" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "fuzzyFileSearch/sessionUpdated".to_string(),
                                            params: Some(serde_json::json!({
                                                "sessionId": "same-session-recovered-primary-fuzzy-session",
                                                "query": "gate",
                                                "files": [{
                                                    "root": "/tmp/worker-a-primary",
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
                                                "sessionId": "same-session-recovered-primary-fuzzy-session",
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
                                                "deltaBase64": "cmVhZHk=",
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
                                pretty_assertions::assert_eq!(process_id, "proc-worker-a");
                                serde_json::json!({})
                            }
                            "command/exec/resize" => {
                                let process_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("processId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("command/exec/resize should include processId");
                                pretty_assertions::assert_eq!(process_id, "proc-worker-a");
                                serde_json::json!({})
                            }
                            "command/exec/terminate" => {
                                let process_id = request
                                    .params
                                    .as_ref()
                                    .and_then(|params| params.get("processId"))
                                    .and_then(serde_json::Value::as_str)
                                    .expect("command/exec/terminate should include processId");
                                pretty_assertions::assert_eq!(process_id, "proc-worker-a");
                                serde_json::json!({})
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
                        match method.as_str() {
                            "account/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "account/updated".to_string(),
                                            params: Some(serde_json::json!({
                                                "authMode": null,
                                                "planType": null,
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "account/rateLimits/read" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "account/rateLimits/updated".to_string(),
                                            params: Some(serde_json::json!({
                                                "rateLimits": {
                                                    "limitId": format!("{worker_label}-notification"),
                                                    "limitName": format!("{worker_label} Notification"),
                                                    "primary": {
                                                        "usedPercent": 77,
                                                        "windowMinutes": 300,
                                                        "resetsAt": 1_700_000_000,
                                                    },
                                                    "secondary": null,
                                                    "credits": null,
                                                    "planType": null,
                                                    "rateLimitReachedType": null,
                                                },
                                            })),
                                        },
                                    ),
                                )
                                .await;
                            }
                            "app/list" => {
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "app/list/updated".to_string(),
                                            params: Some(serde_json::json!({
                                                "data": [{
                                                    "id": format!("{worker_label}-notification-app"),
                                                    "name": format!("{worker_label} Notification App"),
                                                    "description": "Recovered worker app update",
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
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "warning".to_string(),
                                            params: Some(serde_json::json!({
                                                "threadId": null,
                                                "message": format!("{worker_label} recovered warning"),
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "configWarning".to_string(),
                                            params: Some(serde_json::json!({
                                                "summary": format!("{worker_label} recovered config warning"),
                                                "details": "check recovered worker config",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "deprecationNotice".to_string(),
                                            params: Some(serde_json::json!({
                                                "summary": format!("{worker_label} recovered deprecation notice"),
                                                "details": "update recovered worker workflow",
                                            })),
                                        },
                                    ),
                                )
                                .await;
                                write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Notification(
                                        codex_app_server_protocol::JSONRPCNotification {
                                            method: "windows/worldWritableWarning".to_string(),
                                            params: Some(serde_json::json!({
                                                "samplePaths": [format!("C:\\{worker_label}-recovered")],
                                                "extraCount": 1,
                                                "failedScan": false,
                                            })),
                                        },
                                    ),
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
                            _ => {}
                        }
                    }
                }
                _ => unreachable!("unexpected connection index"),
            }
        }
    });
    format!("ws://{addr}")
}
