use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_for_server_requests() {
    let worker_a = start_reconnecting_v2_multi_connection_server_request_server(
        "thread-worker-a-3",
        "/tmp/worker-a-3",
        "safe-a-3",
        "acct-worker-a-3",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_server_request_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "safe-b",
        "acct-worker-b",
        LegacyApprovalExercise::Skip,
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
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
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
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before same-session server-request test starts");

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("remote client should connect to multi-worker gateway");

    let first_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
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
        .expect("same northbound session should reuse recovered worker for thread/start");

    let second_started: AppServerThreadStartResponse = client
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
        .expect("shared session should still round-robin onto surviving worker");

    let expected_threads = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    assert_eq!(
        expected_threads,
        HashSet::from([
            "thread-worker-a-3".to_string(),
            "thread-worker-b".to_string(),
        ])
    );
    let mut gateway_request_ids = HashSet::new();
    let mut user_input_threads = HashSet::new();
    let mut command_threads = HashSet::new();
    let mut file_threads = HashSet::new();
    let mut dynamic_tool_request_ids = HashSet::new();
    let mut dynamic_tool_threads = HashSet::new();
    let mut dynamic_tool_started_threads = HashSet::new();
    let mut dynamic_tool_completed_threads = HashSet::new();
    let mut dynamic_tool_resolved_threads = HashSet::new();
    let mut permissions_threads = HashSet::new();
    let mut mcp_threads = HashSet::new();
    let mut refresh_accounts = HashSet::new();

    timeout(Duration::from_secs(5), async {
        while user_input_threads.len() < expected_threads.len()
            || command_threads.len() < expected_threads.len()
            || file_threads.len() < expected_threads.len()
            || dynamic_tool_threads.len() < expected_threads.len()
            || dynamic_tool_started_threads.len() < expected_threads.len()
            || dynamic_tool_completed_threads.len() < expected_threads.len()
            || dynamic_tool_resolved_threads.len() < expected_threads.len()
            || permissions_threads.len() < expected_threads.len()
            || mcp_threads.len() < expected_threads.len()
            || refresh_accounts.len() < expected_threads.len()
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.item_id, "tool-call-remote-workflow");
                    assert_eq!(params.questions.len(), 1);
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-server-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    let answer = match params.thread_id.as_str() {
                        "thread-worker-a-3" => "safe-a-3",
                        "thread-worker-b" => "safe-b",
                        thread_id => panic!("unexpected thread id: {thread_id}"),
                    };
                    let mut answers = HashMap::new();
                    answers.insert(
                        "mode".to_string(),
                        ToolRequestUserInputAnswer {
                            answers: vec![answer.to_string()],
                        },
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ToolRequestUserInputResponse { answers })
                                .expect("server request response should serialize"),
                        )
                        .await
                        .expect("server request should resolve");
                    user_input_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "cmd-remote-workflow");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-command-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                                decision: CommandExecutionApprovalDecision::Accept,
                            })
                            .expect("command approval response should serialize"),
                        )
                        .await
                        .expect("command approval should resolve");
                    command_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "file-remote-workflow");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-file-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(FileChangeRequestApprovalResponse {
                                decision: FileChangeApprovalDecision::Accept,
                            })
                            .expect("file approval response should serialize"),
                        )
                        .await
                        .expect("file approval should resolve");
                    file_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::DynamicToolCall {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.call_id, "tool-call-remote-workflow");
                    assert_eq!(params.tool, "image-edit");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-dynamic-tool-call-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    assert!(
                        dynamic_tool_request_ids.insert(request_id.clone()),
                        "dynamic tool request ids should stay unique after worker reconnect",
                    );
                    let tool_output = match params.thread_id.as_str() {
                        "thread-worker-a-3" => "tool output for thread-worker-a-3",
                        "thread-worker-b" => "tool output for thread-worker-b",
                        thread_id => panic!("unexpected thread id: {thread_id}"),
                    };
                    assert_eq!(
                        params.arguments,
                        serde_json::json!({
                            "prompt": format!("Sharpen image for {}", params.thread_id),
                            "strength": 0.5,
                        })
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: tool_output.to_string(),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool response should serialize"),
                        )
                        .await
                        .expect("dynamic tool request should resolve");
                    dynamic_tool_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) => {
                    if matches!(
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
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && arguments
                                    == &serde_json::json!({
                                        "prompt": format!(
                                            "Sharpen image for {}",
                                            notification.thread_id
                                        ),
                                        "strength": 0.5,
                                    })
                                && *status == DynamicToolCallStatus::InProgress
                                && content_items.is_none()
                                && success.is_none()
                                && duration_ms.is_none()
                        )
                    {
                        dynamic_tool_started_threads.insert(notification.thread_id);
                    }
                }
                AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "perm-remote-workflow");
                    assert_eq!(params.reason, Some("Need wider permissions".to_string()));
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-permissions-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(PermissionsRequestApprovalResponse {
                                strict_auto_review: None,

                                permissions: codex_app_server_protocol::GrantedPermissionProfile {
                                    network: params.permissions.network,
                                    file_system: params.permissions.file_system,
                                },
                                scope: PermissionGrantScope::Turn,
                            })
                            .expect("permissions approval response should serialize"),
                        )
                        .await
                        .expect("permissions approval should resolve");
                    permissions_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                    assert_eq!(params.server_name, "mock-mcp");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-mcp-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(McpServerElicitationRequestResponse {
                                action: McpServerElicitationAction::Accept,
                                content: Some(serde_json::json!({
                                    "confirmed": true,
                                })),
                                meta: None,
                            })
                            .expect("mcp elicitation response should serialize"),
                        )
                        .await
                        .expect("mcp elicitation should resolve");
                    mcp_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                    request_id,
                    params,
                }) => {
                    let previous_account_id = params
                        .previous_account_id
                        .as_deref()
                        .expect("refresh request should include previous account id");
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-chatgpt-refresh-request".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should stay unique after worker reconnect",
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                access_token: format!("access-token-{previous_account_id}"),
                                chatgpt_account_id: previous_account_id.to_string(),
                                chatgpt_plan_type: Some("pro".to_string()),
                            })
                            .expect("chatgpt refresh response should serialize"),
                        )
                        .await
                        .expect("chatgpt refresh should resolve");
                    refresh_accounts.insert(previous_account_id.to_string());
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    ServerRequestResolvedNotification {
                        thread_id,
                        request_id,
                    },
                )) => {
                    if dynamic_tool_request_ids.contains(&request_id) {
                        dynamic_tool_resolved_threads.insert(thread_id);
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) => {
                    if matches!(
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
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && arguments
                                    == &serde_json::json!({
                                        "prompt": format!(
                                            "Sharpen image for {}",
                                            notification.thread_id
                                        ),
                                        "strength": 0.5,
                                    })
                                && *status == DynamicToolCallStatus::Completed
                                && *content_items
                                    == Some(vec![
                                        DynamicToolCallOutputContentItem::InputText {
                                            text: format!(
                                                "tool output for {}",
                                                notification.thread_id
                                            ),
                                        },
                                    ])
                                && *success == Some(true)
                                && *duration_ms == Some(7)
                        )
                    {
                        dynamic_tool_completed_threads.insert(notification.thread_id);
                    }
                }
                other => panic!("unexpected event: {other:?}"),
            }
        }
    })
    .await
    .expect("server requests should arrive from both workers after reconnect");

    assert_eq!(user_input_threads, expected_threads);
    assert_eq!(command_threads, expected_threads);
    assert_eq!(file_threads, expected_threads);
    assert_eq!(dynamic_tool_threads, expected_threads);
    assert_eq!(dynamic_tool_started_threads, expected_threads);
    assert_eq!(dynamic_tool_completed_threads, expected_threads);
    assert_eq!(dynamic_tool_resolved_threads, expected_threads);
    assert_eq!(permissions_threads, expected_threads);
    assert_eq!(mcp_threads, expected_threads);
    assert_eq!(dynamic_tool_request_ids.len(), expected_threads.len());
    assert_eq!(gateway_request_ids.len(), expected_threads.len() * 7);
    assert_eq!(
        refresh_accounts,
        HashSet::from(["acct-worker-a-3".to_string(), "acct-worker-b".to_string(),]),
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
