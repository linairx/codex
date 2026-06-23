use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_multi_worker_single_worker_bootstrap.rs"]
mod embedded_tests_multi_worker_single_worker_bootstrap;

#[tokio::test]
async fn remote_single_worker_v2_clients_recover_after_worker_reconnect() {
    let websocket_url =
        start_reconnecting_v2_mock_remote_server(Some("secret-token".to_string())).await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: websocket_url.clone(),
                    auth_token: Some("secret-token".to_string()),
                    account_id: None,
                }],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && !worker.reconnecting)
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .and_then(|worker| worker.last_error.as_ref())
                    .is_some()
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");

    let mut second_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 16,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let second_started: AppServerThreadStartResponse = second_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("second thread/start should succeed after worker reconnect");
    assert_eq!(second_started.thread.id, "thread-worker-a-2");

    let mut saw_user_input_request = false;
    let mut saw_user_input_resolved = false;
    let mut saw_command_request = false;
    let mut saw_command_resolved = false;
    let mut saw_file_request = false;
    let mut saw_file_resolved = false;
    let mut saw_mcp_elicitation_request = false;
    let mut saw_mcp_elicitation_resolved = false;
    let mut saw_dynamic_tool_item_started = false;
    let mut saw_dynamic_tool_request = false;
    let mut saw_dynamic_tool_resolved = false;
    let mut saw_dynamic_tool_item_completed = false;
    let mut saw_permissions_request = false;
    let mut saw_permissions_resolved = false;
    let mut saw_refresh_request = false;
    let mut saw_refresh_resolved = false;
    let mut saw_turn_completed = false;
    let mut dynamic_tool_request_id: Option<RequestId> = None;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = second_client
                .next_event()
                .await
                .expect("event stream should stay open");
            tracing::info!(?event, "reconnect client received event");
            match event {
                AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) => {
                    assert_eq!(saw_user_input_request, false);
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, "turn-worker-a-2");
                    assert_eq!(params.item_id, "tool-call-worker-a-2");
                    let mut answers = HashMap::new();
                    answers.insert(
                        "mode".to_string(),
                        ToolRequestUserInputAnswer {
                            answers: vec!["safe".to_string()],
                        },
                    );
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ToolRequestUserInputResponse { answers })
                                .expect("server request response should serialize"),
                        )
                        .await
                        .expect("server request should resolve after worker reconnect");
                    saw_user_input_request = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    ServerRequestResolvedNotification {
                        thread_id,
                        request_id,
                    },
                )) => {
                    if dynamic_tool_request_id.as_ref() == Some(&request_id) {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_dynamic_tool_resolved = true;
                    } else if request_id
                        == RequestId::String("srv-command-after-reconnect".to_string())
                    {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_command_resolved = true;
                    } else if request_id
                        == RequestId::String("srv-file-after-reconnect".to_string())
                    {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_file_resolved = true;
                    } else if request_id
                        == RequestId::String("srv-mcp-elicitation-after-reconnect".to_string())
                    {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_mcp_elicitation_resolved = true;
                    } else if request_id
                        == RequestId::String("srv-permissions-after-reconnect".to_string())
                    {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_permissions_resolved = true;
                    } else if request_id
                        == RequestId::String("srv-chatgpt-refresh-after-reconnect".to_string())
                    {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        saw_refresh_resolved = true;
                    } else {
                        assert_eq!(thread_id, "thread-worker-a-2");
                        assert_eq!(
                            request_id,
                            RequestId::String("srv-user-input-after-reconnect".to_string())
                        );
                        saw_user_input_resolved = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == "thread-worker-a-2"
                    && notification.turn_id == "turn-worker-a-2"
                    && matches!(
                                &notification.item,
                                ThreadItem::DynamicToolCall {
                    namespace: None,
                                    id,
                                    tool,
                                    arguments,
                                    status,
                                    content_items,
                                    success,
                                    duration_ms,
                                } if id == "tool-call-after-reconnect"
                                    && tool == "image-edit"
                                    && *arguments
                                        == serde_json::json!({
                                            "prompt": "Sharpen image after reconnect",
                                            "strength": 0.75,
                                        })
                                    && *status == DynamicToolCallStatus::InProgress
                                    && content_items.is_none()
                                    && success.is_none()
                                    && duration_ms.is_none()
                            ) =>
                {
                    saw_dynamic_tool_item_started = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, "turn-worker-a-2");
                    assert_eq!(params.item_id, "cmd-worker-a-2");
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                                decision: CommandExecutionApprovalDecision::Accept,
                            })
                            .expect("command approval response should serialize"),
                        )
                        .await
                        .expect("command approval should resolve after reconnect");
                    saw_command_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, "turn-worker-a-2");
                    assert_eq!(params.item_id, "file-worker-a-2");
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(FileChangeRequestApprovalResponse {
                                decision: FileChangeApprovalDecision::Accept,
                            })
                            .expect("file approval response should serialize"),
                        )
                        .await
                        .expect("file approval should resolve after reconnect");
                    saw_file_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, Some("turn-worker-a-2".to_string()));
                    assert_eq!(params.server_name, "mock-mcp");
                    match params.request {
                        McpServerElicitationRequest::Form {
                            message,
                            requested_schema,
                            ..
                        } => {
                            assert_eq!(message, "Allow reconnect action?");
                            assert_eq!(
                                serde_json::to_value(requested_schema)
                                    .expect("schema should serialize"),
                                serde_json::json!({
                                    "type": "object",
                                    "properties": {
                                        "confirmed": {
                                            "type": "boolean",
                                        },
                                    },
                                    "required": ["confirmed"],
                                })
                            );
                        }
                        other => panic!("unexpected reconnect elicitation request: {other:?}"),
                    }
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::json!({
                                "action": "accept",
                                "content": {
                                    "confirmed": true,
                                },
                                "_meta": null,
                            }),
                        )
                        .await
                        .expect("mcp elicitation should resolve after reconnect");
                    saw_mcp_elicitation_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::DynamicToolCall {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, "turn-worker-a-2");
                    dynamic_tool_request_id = Some(request_id.clone());
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: "tool output after reconnect".to_string(),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool response should serialize"),
                        )
                        .await
                        .expect("dynamic tool request should resolve after reconnect");
                    saw_dynamic_tool_request = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == "thread-worker-a-2"
                    && notification.turn_id == "turn-worker-a-2"
                    && matches!(
                                &notification.item,
                                ThreadItem::DynamicToolCall {
                    namespace: None,
                                    id,
                                    tool,
                                    arguments,
                                    status,
                                    content_items,
                                    success,
                                    duration_ms,
                                } if id == "tool-call-after-reconnect"
                                    && tool == "image-edit"
                                    && *arguments
                                        == serde_json::json!({
                                            "prompt": "Sharpen image after reconnect",
                                            "strength": 0.75,
                                        })
                                    && *status == DynamicToolCallStatus::Completed
                                    && *content_items
                                        == Some(vec![
                                            DynamicToolCallOutputContentItem::InputText {
                                                text: "tool output after reconnect".to_string(),
                                            },
                                        ])
                                    && *success == Some(true)
                                    && *duration_ms == Some(9)
                            ) =>
                {
                    saw_dynamic_tool_item_completed = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-worker-a-2");
                    assert_eq!(params.turn_id, "turn-worker-a-2");
                    assert_eq!(params.item_id, "perm-worker-a-2");
                    assert_eq!(
                        params.reason,
                        Some("Need reconnect permissions".to_string())
                    );
                    assert_eq!(
                        serde_json::to_value(params.permissions)
                            .expect("permissions should serialize"),
                        serde_json::json!({
                            "fileSystem": null,
                            "network": {
                                "enabled": true,
                            },
                        })
                    );
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::json!({
                                "permissions": {
                                    "fileSystem": null,
                                    "network": {
                                        "enabled": true,
                                    },
                                },
                                "scope": "turn",
                            }),
                        )
                        .await
                        .expect("permissions approval should resolve after reconnect");
                    saw_permissions_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                    request_id,
                    params,
                }) => {
                    assert_eq!(
                        params.previous_account_id,
                        Some("acct-reconnect".to_string())
                    );
                    second_client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                access_token: "access-token-reconnect".to_string(),
                                chatgpt_account_id: "acct-reconnect".to_string(),
                                chatgpt_plan_type: Some("pro".to_string()),
                            })
                            .expect("chatgpt refresh response should serialize"),
                        )
                        .await
                        .expect("chatgpt refresh should resolve after reconnect");
                    saw_refresh_request = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    TurnCompletedNotification { thread_id, turn },
                )) => {
                    assert_eq!(thread_id, "thread-worker-a-2");
                    assert_eq!(turn.id, "turn-worker-a-2");
                    assert_eq!(turn.status, TurnStatus::Completed);
                    saw_turn_completed = true;
                }
                _ => {}
            }
            if saw_turn_completed
                && saw_user_input_resolved
                && saw_command_resolved
                && saw_file_resolved
                && saw_mcp_elicitation_resolved
                && saw_dynamic_tool_resolved
                && saw_permissions_resolved
                && saw_refresh_resolved
            {
                break;
            }
        }
    })
    .await
    .expect("recovered turn should finish after dynamic tool lifecycle");
    assert_eq!(saw_user_input_resolved, true);
    assert_eq!(saw_command_resolved, true);
    assert_eq!(saw_file_resolved, true);
    assert_eq!(saw_mcp_elicitation_resolved, true);
    assert_eq!(saw_dynamic_tool_resolved, true);
    assert_eq!(saw_permissions_resolved, true);
    assert_eq!(saw_refresh_resolved, true);

    let healthz_response = client
        .get(format!("http://{}/healthz", server.local_addr()))
        .send()
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.status, GatewayHealthStatus::Ok);
    assert_eq!(
        health.v2_compatibility,
        GatewayV2CompatibilityMode::RemoteSingleWorker
    );

    drop(events_response);
    assert_remote_client_shutdown(second_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_thread_control_and_review_workflow_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_single_worker_thread_control_server(
        "thread-single-worker-reconnected",
        "/tmp/single-worker-reconnected",
    )
    .await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url,
                    auth_token: None,
                    account_id: None,
                }],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && worker.last_error.is_some())
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before thread-control recovery test starts");

    let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect after worker reconnect");

    sleep(Duration::from_millis(200)).await;

    let started: AppServerThreadStartResponse = timeout(Duration::from_secs(5), async {
        let mut attempt = 1;
        loop {
            match client
                .request_typed(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(attempt),
                    params: ThreadStartParams {
                        model: None,
                        model_provider: None,
                        service_tier: None,
                        cwd: Some("/tmp/single-worker-reconnected".to_string()),
                        approval_policy: None,
                        approvals_reviewer: None,
                        sandbox: None,
                        config: None,
                        service_name: None,
                        base_instructions: None,
                        developer_instructions: None,
                        personality: None,
                        ephemeral: Some(true),
                        session_start_source: None,
                        dynamic_tools: None,
                        mock_experimental_field: None,
                        experimental_raw_events: false,
                        ..Default::default()
                    },
                })
                .await
            {
                Ok(started) => break started,
                Err(TypedRequestError::Transport { source, .. })
                    if source.kind() == ErrorKind::BrokenPipe =>
                {
                    attempt += 1;
                    sleep(Duration::from_millis(100)).await;
                }
                Err(err) => panic!("thread/start should succeed after worker reconnect: {err}"),
            }
        }
    })
    .await
    .expect("thread/start retry window should finish in time");
    assert_eq!(started.thread.id, "thread-single-worker-reconnected");

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed after worker reconnect");
    assert_eq!(
        unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(3),
            params: ThreadArchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed after worker reconnect");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(4),
            params: ThreadUnarchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed after worker reconnect");
    assert_eq!(unarchive.thread.id, started.thread.id);
    assert_eq!(unarchive.thread.preview, "/tmp/single-worker-reconnected");

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(5),
            params: ThreadMetadataUpdateParams {
                thread_id: started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("reconnected-sha".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed after worker reconnect");
    assert_eq!(metadata_update.thread.id, started.thread.id);

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(6),
            params: ThreadTurnsListParams {
                thread_id: started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed after worker reconnect");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, "turn-thread-single-worker-reconnected");
    assert_eq!(turns.next_cursor, None);
    assert_eq!(turns.backwards_cursor, None);

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(7),
            params: ThreadIncrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed after worker reconnect");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(8),
            params: ThreadDecrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed after worker reconnect");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(9),
            params: ThreadInjectItemsParams {
                thread_id: started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "reconnected seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed after worker reconnect");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(10),
            params: ThreadCompactStartParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed after worker reconnect");
    assert_eq!(compact, ThreadCompactStartResponse {});

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(11),
            params: ThreadShellCommandParams {
                thread_id: started.thread.id.clone(),
                command: "printf single-worker-reconnected".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed after worker reconnect");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(12),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed after worker reconnect");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(13),
            params: ThreadRollbackParams {
                thread_id: started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed after worker reconnect");
    assert_eq!(rollback.thread.id, started.thread.id);
    assert_eq!(rollback.thread.preview, "/tmp/single-worker-reconnected");

    let review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(14),
            params: ReviewStartParams {
                thread_id: started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review after reconnect".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should succeed after worker reconnect");
    assert_eq!(
        review.turn.id,
        "turn-review-thread-single-worker-reconnected"
    );
    assert_eq!(
        review.review_thread_id,
        "thread-single-worker-reconnected-review"
    );

    let review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(15),
            params: ThreadReadParams {
                thread_id: review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for review thread after worker reconnect");
    assert_eq!(review_thread.thread.id, review.review_thread_id);
    assert_eq!(
        review_thread.thread.preview,
        "/tmp/single-worker-reconnected"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_v2_realtime_workflow_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_multi_connection_realtime_server(
        "thread-worker-a-2",
        "/tmp/worker-a-2",
        "session-worker-a-2",
        "delta after reconnect",
        "done after reconnect",
    )
    .await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url,
                    auth_token: None,
                    account_id: None,
                }],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && !worker.reconnecting)
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .and_then(|worker| worker.last_error.as_ref())
                    .is_some()
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");

    let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 64,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let voices: ThreadRealtimeListVoicesResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeListVoices {
            request_id: RequestId::Integer(1),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed after worker reconnect");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList::builtin(),
        }
    );

    let started: AppServerThreadStartResponse = v2_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed after worker reconnect");
    assert_eq!(started.thread.id, "thread-worker-a-2");

    let realtime_started: ThreadRealtimeStartResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: started.thread.id.clone(),
                output_modality: RealtimeOutputModality::Text,
                prompt: None,
                realtime_session_id: None,
                transport: None,
                voice: None,
                client_managed_handoffs: None,
                model: None,
                version: None,
                codex_responses_as_items: None,
                codex_response_item_prefix: None,
                codex_response_handoff_prefix: None,
                include_startup_context: None,
            },
        })
        .await
        .expect("thread/realtime/start should succeed after worker reconnect");
    assert_eq!(realtime_started, ThreadRealtimeStartResponse {});

    let append_text: ThreadRealtimeAppendTextResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeAppendTextParams {
                thread_id: started.thread.id.clone(),
                text: "hello reconnect realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should succeed after worker reconnect");
    assert_eq!(append_text, ThreadRealtimeAppendTextResponse {});

    let append_audio: ThreadRealtimeAppendAudioResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-reconnect-audio".to_string()),
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should succeed after worker reconnect");
    assert_eq!(append_audio, ThreadRealtimeAppendAudioResponse {});

    let stop_response: ThreadRealtimeStopResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeStopParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should succeed after worker reconnect");
    assert_eq!(stop_response, ThreadRealtimeStopResponse {});

    let mut coverage = RealtimeStreamingCoverage::default();
    timeout(Duration::from_secs(5), async {
        while coverage
            != (RealtimeStreamingCoverage {
                saw_started: true,
                saw_item_added: true,
                saw_output_audio_delta: true,
                saw_transcript_delta: true,
                saw_transcript_done: true,
                saw_sdp: true,
                saw_error: true,
                saw_closed: true,
            })
        {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.realtime_session_id.as_deref()
                        == Some("session-worker-a-2")
                    && notification.version == RealtimeConversationVersion::V2 =>
                {
                    coverage.saw_started = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.delta == "delta after reconnect" =>
                {
                    coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.role == "assistant"
                    && notification.text == "done after reconnect" =>
                {
                    coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == started.thread.id
                    && notification.audio.data == "AQID"
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1 =>
                {
                    coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.message == "realtime transport warning" =>
                {
                    coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.reason.as_deref() == Some("client requested stop") =>
                {
                    coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime workflow notifications should arrive after reconnect");

    assert_eq!(
        coverage,
        RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        }
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
