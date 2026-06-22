use super::*;

#[path = "embedded_test_support_multi_connection_disconnect_reconnect.rs"]
mod embedded_test_support_multi_connection_disconnect_reconnect;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_connection_disconnect_reconnect::*;

#[path = "embedded_test_support_multi_connection_disconnect_server_requests.rs"]
mod embedded_test_support_multi_connection_disconnect_server_requests;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_connection_disconnect_server_requests::*;

#[path = "embedded_test_support_multi_connection_disconnect_wrong_thread_read.rs"]
mod embedded_test_support_multi_connection_disconnect_wrong_thread_read;

#[allow(unused_imports)]
pub(crate) use self::embedded_test_support_multi_connection_disconnect_wrong_thread_read::*;

pub(crate) async fn start_mock_remote_multi_connection_session_mutation_server(
    worker_label: &'static str,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let received_methods = Arc::new(Mutex::new(Vec::new()));
    let received_methods_for_task = Arc::clone(&received_methods);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let received_methods = Arc::clone(&received_methods_for_task);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    received_methods.lock().await.push(request.method.clone());

                    let result = match request.method.as_str() {
                        "externalAgentConfig/import" => {
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
                            serde_json::json!({
                                "importId": "import-1",
                            })
                        }
                        "config/value/write" => serde_json::json!({
                            "status": "ok",
                            "version": worker_label,
                            "filePath": "/tmp/shared/config.toml",
                            "overriddenMetadata": null,
                        }),
                        "config/batchWrite" => serde_json::json!({
                            "status": "ok",
                            "version": worker_label,
                            "filePath": "/tmp/shared/config.toml",
                            "overriddenMetadata": null,
                        }),
                        "marketplace/add" => serde_json::json!({
                            "marketplaceName": "shared-marketplace",
                            "installedRoot": "/tmp/shared/marketplace",
                            "alreadyAdded": false,
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
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "skills/changed".to_string(),
                                        params: Some(serde_json::json!({})),
                                    },
                                ),
                            )
                            .await;
                            serde_json::json!({
                                "effectiveEnabled": true,
                            })
                        }
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
                        "config/mcpServer/reload" => serde_json::json!({}),
                        "fs/watch" => {
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
                                                "watchId": "watch-shared",
                                                "changedPaths": [
                                                    format!("/tmp/{worker_label}/shared-repo/.git/HEAD"),
                                                ],
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
                        "memory/reset" => serde_json::json!({}),
                        "account/logout" => {
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
                }
            });
        }
    });
    (format!("ws://{addr}"), received_methods)
}

pub(crate) async fn start_mock_remote_multi_connection_discovery_server(
    worker_label: &'static str,
    shared_cwd: &'static str,
    unique_cwd: &'static str,
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

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenAiAuthRequirement {
    NotRequired,
    Required,
}

pub(crate) async fn start_mock_remote_multi_connection_bootstrap_server(
    openai_auth_requirement: OpenAiAuthRequirement,
    rate_limits: Option<Vec<(&'static str, &'static str, i32)>>,
    models: Vec<(&'static str, &'static str, bool)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let models = models.clone();
            let rate_limits = rate_limits.clone();
            let requires_openai_auth =
                matches!(openai_auth_requirement, OpenAiAuthRequirement::Required);
            tokio::spawn(async move {
                let mut websocket = tokio_tungstenite::accept_async(stream)
                    .await
                    .expect("websocket upgrade should succeed");
                expect_remote_initialize(&mut websocket).await;

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "account/read" => serde_json::json!({
                            "account": null,
                            "requiresOpenaiAuth": requires_openai_auth,
                        }),
                        "getAuthStatus" => serde_json::json!({
                            "authMethod": null,
                            "authToken": "worker-a-token",
                            "requiresOpenaiAuth": requires_openai_auth,
                        }),
                        "account/rateLimits/read" => {
                            let snapshots = rate_limits
                                .as_ref()
                                .expect("rate limits should be configured for bootstrap test");
                            let primary = snapshots
                                .first()
                                .expect("rate limits should include a primary snapshot");
                            let rate_limits_by_limit_id = snapshots
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
                        "model/list" => paginated_model_list_result(&request, &models),
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

pub(crate) async fn start_mock_remote_multi_connection_legacy_server(
    worker_label: &'static str,
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

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    match request.method.as_str() {
                        "thread/start" => {
                            let (thread_id, preview, rollout_path) = if worker_label == "worker-b" {
                                (
                                    "00000000-0000-0000-0000-0000000000b2",
                                    "/tmp/worker-b",
                                    "/tmp/worker-b/rollout.jsonl",
                                )
                            } else {
                                (
                                    "00000000-0000-0000-0000-0000000000a1",
                                    "/tmp/worker-a",
                                    "/tmp/worker-a/rollout.jsonl",
                                )
                            };
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "thread": mock_thread_with_path(
                                            thread_id,
                                            preview,
                                            Some(rollout_path),
                                        ),
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
                        }
                        "thread/list" => {
                            let data = if worker_label == "worker-b" {
                                vec![mock_thread_with_path(
                                    "00000000-0000-0000-0000-0000000000b2",
                                    "/tmp/worker-b",
                                    Some("/tmp/worker-b/rollout.jsonl"),
                                )]
                            } else {
                                Vec::new()
                            };
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "data": data,
                                        "nextCursor": null,
                                    }),
                                }),
                            )
                            .await;
                        }
                        "getConversationSummary" if worker_label == "worker-b" => {
                            write_websocket_message(
                                    &mut websocket,
                                    JSONRPCMessage::Response(JSONRPCResponse {
                                        id: request.id,
                                        result: serde_json::json!({
                                            "summary": {
                                                "conversationId": "00000000-0000-0000-0000-0000000000b2",
                                                "path": "/tmp/worker-b/rollout.jsonl",
                                                "preview": "Worker B summary",
                                                "timestamp": null,
                                                "updatedAt": null,
                                                "modelProvider": "openai",
                                                "cwd": "/tmp/worker-b",
                                                "cliVersion": "0.0.0-test",
                                                "source": "codex_cli",
                                                "gitInfo": null,
                                            },
                                        }),
                                    }),
                                )
                                .await;
                        }
                        "gitDiffToRemote" if worker_label == "worker-b" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "sha": "0123456789abcdef0123456789abcdef01234567",
                                        "diff": "diff --git a/README.md b/README.md\n",
                                    }),
                                }),
                            )
                            .await;
                        }
                        "getConversationSummary" | "gitDiffToRemote" => {
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Error(JSONRPCError {
                                    id: request.id,
                                    error: JSONRPCErrorError {
                                        code: -32000,
                                        message: format!(
                                            "{worker_label} cannot serve {}",
                                            request.method
                                        ),
                                        data: None,
                                    },
                                }),
                            )
                            .await;
                        }
                        method => panic!("unexpected legacy request method: {method}"),
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerAAccountCapacity {
    Available,
    Exhausted,
}

pub(crate) async fn start_mock_remote_multi_connection_legacy_account_handoff_server(
    worker_label: &'static str,
    worker_a_account_capacity: WorkerAAccountCapacity,
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

                loop {
                    let request = read_websocket_request(&mut websocket).await;
                    let result = match request.method.as_str() {
                        "thread/start" => {
                            let (thread_id, preview, rollout_path) = if worker_label == "worker-b" {
                                (
                                    "00000000-0000-0000-0000-0000000000b2",
                                    "/tmp/worker-b",
                                    "/tmp/worker-b/rollout.jsonl",
                                )
                            } else {
                                (
                                    "00000000-0000-0000-0000-0000000000a1",
                                    "/tmp/worker-a",
                                    "/tmp/worker-a/rollout.jsonl",
                                )
                            };
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    thread_id,
                                    preview,
                                    Some(rollout_path),
                                ),
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
                        "account/rateLimits/read" => {
                            let (limit_id, limit_name, used_percent, reached_type) = if worker_label
                                == "worker-b"
                            {
                                ("worker-b", "Worker B", 100, Some("rate_limit_reached"))
                            } else if worker_a_account_capacity == WorkerAAccountCapacity::Exhausted
                            {
                                ("worker-a", "Worker A", 100, Some("rate_limit_reached"))
                            } else {
                                ("codex", "Codex", 20, None)
                            };
                            serde_json::json!({
                                "rateLimits": {
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
                                    "rateLimitReachedType": reached_type,
                                },
                                "rateLimitsByLimitId": {
                                    limit_id: {
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
                                        "rateLimitReachedType": reached_type,
                                    },
                                },
                            })
                        }
                        "getConversationSummary" if worker_label == "worker-a" => {
                            let conversation_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("conversationId"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("00000000-0000-0000-0000-0000000000a1");
                            serde_json::json!({
                                "summary": {
                                    "conversationId": conversation_id,
                                    "path": "/tmp/worker-b/rollout.jsonl",
                                    "preview": "Worker A restored summary",
                                    "timestamp": null,
                                    "updatedAt": null,
                                    "modelProvider": "openai",
                                    "cwd": "/tmp/worker-a",
                                    "cliVersion": "0.0.0-test",
                                    "source": "codex_cli",
                                    "gitInfo": null,
                                },
                            })
                        }
                        "thread/resume" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/resume should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    requested_thread_id,
                                    "/tmp/worker-a",
                                    Some("/tmp/worker-b/rollout.jsonl"),
                                ),
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/worker-a",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/fork" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/fork should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": {
                                    "id": "00000000-0000-0000-0000-0000000000a2",
                                    "sessionId": "00000000-0000-0000-0000-0000000000a2",
                                    "forkedFromId": requested_thread_id,
                                    "preview": "/tmp/worker-a-fork",
                                    "ephemeral": true,
                                    "modelProvider": "openai",
                                    "createdAt": 1,
                                    "updatedAt": 1,
                                    "status": {
                                        "type": "idle"
                                    },
                                    "path": "/tmp/worker-b/forked-rollout.jsonl",
                                    "cwd": "/tmp/worker-a-fork",
                                    "cliVersion": "0.0.0-test",
                                    "source": "codex_cli",
                                    "agentNickname": null,
                                    "agentRole": null,
                                    "gitInfo": null,
                                    "name": null,
                                    "turns": [],
                                },
                                "model": "gpt-5",
                                "modelProvider": "openai",
                                "serviceTier": null,
                                "cwd": "/tmp/worker-a-fork",
                                "instructionSources": [],
                                "approvalPolicy": "never",
                                "approvalsReviewer": "user",
                                "sandbox": {
                                    "type": "dangerFullAccess"
                                },
                                "reasoningEffort": null,
                            })
                        }
                        "thread/read" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/read should include threadId");
                            assert!(
                                requested_thread_id == "00000000-0000-0000-0000-0000000000b2"
                                    || requested_thread_id
                                        == "00000000-0000-0000-0000-0000000000a2",
                                "thread/read should target a restored thread"
                            );
                            let preview =
                                if requested_thread_id == "00000000-0000-0000-0000-0000000000a2" {
                                    "/tmp/worker-a-fork"
                                } else {
                                    "/tmp/worker-a-read"
                                };
                            let rollout_path =
                                if requested_thread_id == "00000000-0000-0000-0000-0000000000a2" {
                                    "/tmp/worker-b/forked-rollout.jsonl"
                                } else {
                                    "/tmp/worker-b/rollout.jsonl"
                                };
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    requested_thread_id,
                                    preview,
                                    Some(rollout_path),
                                ),
                            })
                        }
                        "thread/rollback" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/rollback should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    requested_thread_id,
                                    "/tmp/worker-a-rollback",
                                    Some("/tmp/worker-b/rollout.jsonl"),
                                ),
                            })
                        }
                        "thread/archive" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/archive should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({})
                        }
                        "thread/unarchive" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/unarchive should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    requested_thread_id,
                                    "/tmp/worker-a-unarchive",
                                    Some("/tmp/worker-b/rollout.jsonl"),
                                ),
                            })
                        }
                        "thread/metadata/update" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/metadata/update should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "thread": mock_thread_with_path(
                                    requested_thread_id,
                                    "/tmp/worker-a-metadata",
                                    Some("/tmp/worker-b/rollout.jsonl"),
                                ),
                            })
                        }
                        "thread/turns/list" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/turns/list should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "data": [mock_turn("turn-worker-a-restored", "completed")],
                                "nextCursor": null,
                                "backwardsCursor": null,
                            })
                        }
                        "thread/increment_elicitation" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/increment_elicitation should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "count": 7,
                                "paused": true,
                            })
                        }
                        "thread/decrement_elicitation" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/decrement_elicitation should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "count": 6,
                                "paused": false,
                            })
                        }
                        "thread/inject_items" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/inject_items should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({})
                        }
                        "thread/name/set" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/name/set should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({})
                        }
                        "thread/memoryMode/set" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/memoryMode/set should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({})
                        }
                        "thread/unsubscribe" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/unsubscribe should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({
                                "status": "unsubscribed",
                            })
                        }
                        "thread/compact/start" if worker_label == "worker-a" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("thread/compact/start should include threadId");
                            pretty_assertions::assert_eq!(
                                requested_thread_id,
                                "00000000-0000-0000-0000-0000000000b2"
                            );
                            serde_json::json!({})
                        }
                        "turn/start" => {
                            let requested_thread_id = request
                                .params
                                .as_ref()
                                .and_then(|params| params.get("threadId"))
                                .and_then(serde_json::Value::as_str)
                                .expect("turn/start should include threadId");
                            let turn_id = format!("turn-{requested_thread_id}");
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Response(JSONRPCResponse {
                                    id: request.id,
                                    result: serde_json::json!({
                                        "turn": mock_turn(&turn_id, "inProgress"),
                                    }),
                                }),
                            )
                            .await;
                            write_websocket_message(
                                &mut websocket,
                                JSONRPCMessage::Notification(
                                    codex_app_server_protocol::JSONRPCNotification {
                                        method: "thread/status/changed".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": requested_thread_id,
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
                                        method: "turn/started".to_string(),
                                        params: Some(serde_json::json!({
                                            "threadId": requested_thread_id,
                                            "turn": mock_turn(&turn_id, "inProgress"),
                                        })),
                                    },
                                ),
                            )
                            .await;
                            continue;
                        }
                        "thread/resume" => {
                            panic!("exhausted worker should not receive thread/resume")
                        }
                        "thread/fork" => {
                            panic!("exhausted worker should not receive thread/fork")
                        }
                        "thread/read" => {
                            panic!("exhausted worker should not receive thread/read")
                        }
                        "thread/rollback" => {
                            panic!("exhausted worker should not receive thread/rollback")
                        }
                        "thread/archive" => {
                            panic!("exhausted worker should not receive thread/archive")
                        }
                        "thread/unarchive" => {
                            panic!("exhausted worker should not receive thread/unarchive")
                        }
                        "thread/metadata/update" => {
                            panic!("exhausted worker should not receive thread/metadata/update")
                        }
                        "thread/turns/list" => {
                            panic!("exhausted worker should not receive thread/turns/list")
                        }
                        "thread/increment_elicitation" => {
                            panic!(
                                "exhausted worker should not receive thread/increment_elicitation"
                            )
                        }
                        "thread/decrement_elicitation" => {
                            panic!(
                                "exhausted worker should not receive thread/decrement_elicitation"
                            )
                        }
                        "thread/inject_items" => {
                            panic!("exhausted worker should not receive thread/inject_items")
                        }
                        "thread/name/set" => {
                            panic!("exhausted worker should not receive thread/name/set")
                        }
                        "thread/memoryMode/set" => {
                            panic!("exhausted worker should not receive thread/memoryMode/set")
                        }
                        "getConversationSummary" => {
                            serde_json::json!({
                                "summary": {
                                    "conversationId": "00000000-0000-0000-0000-0000000000b2",
                                    "path": "/tmp/worker-b/rollout.jsonl",
                                    "preview": "Worker B exhausted summary",
                                    "timestamp": null,
                                    "updatedAt": null,
                                    "modelProvider": "openai",
                                    "cwd": "/tmp/worker-b",
                                    "cliVersion": "0.0.0-test",
                                    "source": "codex_cli",
                                    "gitInfo": null,
                                },
                            })
                        }
                        method => panic!("unexpected legacy handoff request method: {method}"),
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
