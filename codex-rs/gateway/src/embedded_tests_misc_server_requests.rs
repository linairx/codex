use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_server_request_routing_and_id_translation() {
    let worker_a = start_mock_remote_multi_connection_server_request_server(
        "018f0000-0000-7000-8000-0000000000a1",
        "/tmp/worker-a",
        "safe-a",
        "acct-worker-a",
        LegacyApprovalExercise::Exercise,
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_server_request_server(
        "018f0000-0000-7000-8000-0000000000b2",
        "/tmp/worker-b",
        "safe-b",
        "acct-worker-b",
        LegacyApprovalExercise::Exercise,
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
        channel_capacity: 8,
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
        .expect("first thread/start should succeed through multi-worker remote gateway");
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
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let expected_threads = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let mut gateway_request_ids = HashSet::new();
    let mut user_input_threads = HashSet::new();
    let mut command_threads = HashSet::new();
    let mut file_threads = HashSet::new();
    let mut permissions_threads = HashSet::new();
    let mut mcp_threads = HashSet::new();
    let mut dynamic_tool_call_threads = HashSet::new();
    let mut refresh_accounts = HashSet::new();
    let mut legacy_exec_threads = HashSet::new();
    let mut legacy_patch_threads = HashSet::new();
    timeout(Duration::from_secs(5), async {
        while user_input_threads.len() < expected_threads.len()
            || command_threads.len() < expected_threads.len()
            || file_threads.len() < expected_threads.len()
            || permissions_threads.len() < expected_threads.len()
            || mcp_threads.len() < expected_threads.len()
            || dynamic_tool_call_threads.len() < expected_threads.len()
            || refresh_accounts.len() < expected_threads.len()
            || legacy_exec_threads.len() < expected_threads.len()
            || legacy_patch_threads.len() < expected_threads.len()
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
                        "gateway request ids should be unique across workers and methods"
                    );
                    let answer = if params.thread_id == first_started.thread.id {
                        "safe-a"
                    } else if params.thread_id == second_started.thread.id {
                        "safe-b"
                    } else {
                        panic!("unexpected thread id: {}", params.thread_id);
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
                        "gateway request ids should be unique across workers and methods"
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
                        "gateway request ids should be unique across workers and methods"
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
                        "gateway request ids should be unique across workers and methods"
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
                        "gateway request ids should be unique across workers and methods"
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
                        "gateway request ids should be unique across workers and methods"
                    );
                    assert_eq!(
                        params.arguments,
                        serde_json::json!({
                            "prompt": format!("Sharpen image for {}", params.thread_id),
                            "strength": 0.5,
                        })
                    );
                    let thread_id = params.thread_id.clone();
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: format!("tool output for {thread_id}"),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool call response should serialize"),
                        )
                        .await
                        .expect("dynamic tool call should resolve");
                    dynamic_tool_call_threads.insert(params.thread_id);
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) => {
                    assert_eq!(notification.turn_id, "turn-remote-workflow");
                    assert!(matches!(
                            &notification.item,
                            ThreadItem::DynamicToolCall {
                    namespace: None,
                                id,
                                tool,
                                status,
                                ..
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && *status == DynamicToolCallStatus::InProgress
                        ));
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) => {
                    assert_eq!(notification.turn_id, "turn-remote-workflow");
                    assert!(matches!(
                            &notification.item,
                            ThreadItem::DynamicToolCall {
                    namespace: None,
                                id,
                                tool,
                                status,
                                success,
                                ..
                            } if id == "tool-call-remote-workflow"
                                && tool == "image-edit"
                                && *status == DynamicToolCallStatus::Completed
                                && *success == Some(true)
                        ));
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
                        "gateway request ids should be unique across workers and methods"
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
                AppServerEvent::ServerRequest(ServerRequest::ExecCommandApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.call_id, "legacy-exec-call");
                    assert_eq!(params.approval_id, Some("legacy-exec-approval".to_string()));
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-legacy-exec-approval".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ExecCommandApprovalResponse {
                                decision: ReviewDecision::Approved,
                            })
                            .expect("legacy exec approval response should serialize"),
                        )
                        .await
                        .expect("legacy exec approval should resolve");
                    legacy_exec_threads.insert(params.conversation_id.to_string());
                }
                AppServerEvent::ServerRequest(ServerRequest::ApplyPatchApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.call_id, "legacy-patch-call");
                    assert_eq!(
                        params.reason,
                        Some("Need legacy patch approval".to_string())
                    );
                    assert_ne!(
                        request_id,
                        RequestId::String("shared-legacy-patch-approval".to_string())
                    );
                    assert!(
                        gateway_request_ids.insert(request_id.clone()),
                        "gateway request ids should be unique across workers and methods"
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ApplyPatchApprovalResponse {
                                decision: ReviewDecision::ApprovedForSession,
                            })
                            .expect("legacy apply-patch approval response should serialize"),
                        )
                        .await
                        .expect("legacy apply-patch approval should resolve");
                    legacy_patch_threads.insert(params.conversation_id.to_string());
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    _,
                )) => {}
                other => panic!("unexpected event: {other:?}"),
            }
        }
    })
    .await
    .expect("server requests should arrive from both workers");
    assert_eq!(user_input_threads, expected_threads);
    assert_eq!(command_threads, expected_threads);
    assert_eq!(file_threads, expected_threads);
    assert_eq!(permissions_threads, expected_threads);
    assert_eq!(mcp_threads, expected_threads);
    assert_eq!(dynamic_tool_call_threads, expected_threads);
    assert_eq!(
        refresh_accounts,
        HashSet::from(["acct-worker-a".to_string(), "acct-worker-b".to_string(),])
    );
    assert_eq!(legacy_exec_threads, expected_threads);
    assert_eq!(legacy_patch_threads, expected_threads);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_concurrent_overlapping_server_request_ids_over_v2() {
    let request_barrier = Arc::new(Barrier::new(2));
    let worker_a = start_mock_remote_multi_connection_concurrent_server_request_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "safe-a",
        request_barrier.clone(),
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_concurrent_server_request_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "safe-b",
        request_barrier,
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
        channel_capacity: 8,
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
        .expect("first thread/start should succeed through multi-worker remote gateway");
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
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let expected_threads = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let health_client = reqwest::Client::new();
    macro_rules! assert_concurrent_pending_server_request_health {
        (
                $stage:literal,
                $backlog_count:expr,
                $answered_count:expr,
                $worker_answered_count:expr,
                $method_counts:expr
            ) => {{
            let mut last_health = None;
            timeout(Duration::from_secs(5), async {
                loop {
                    let response = health_client
                        .get(format!("http://{}/healthz", server.local_addr()))
                        .send()
                        .await
                        .expect("healthz response");
                    let health: GatewayHealthResponse = response.json().await.expect("health body");
                    if health
                        .v2_connections
                        .active_connection_server_request_backlog_method_counts
                        == $method_counts
                    {
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_max_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_peak_server_request_backlog_count,
                            $backlog_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_started_at
                                .is_some(),
                            true
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_answered_but_unresolved_server_request_count,
                            $answered_count
                        );
                        assert_eq!(
                            health
                                .v2_connections
                                .active_connection_server_request_backlog_worker_counts,
                            vec![
                                GatewayV2ServerRequestBacklogWorkerCounts {
                                    worker_id: Some(0),
                                    pending_server_request_count: 1,
                                    answered_but_unresolved_server_request_count:
                                        $worker_answered_count,
                                    server_request_backlog_count: 1 + $worker_answered_count,
                                },
                                GatewayV2ServerRequestBacklogWorkerCounts {
                                    worker_id: Some(1),
                                    pending_server_request_count: 1,
                                    answered_but_unresolved_server_request_count:
                                        $worker_answered_count,
                                    server_request_backlog_count: 1 + $worker_answered_count,
                                },
                            ]
                        );
                        break;
                    }
                    last_health = Some(health);
                    sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "concurrent server-request health should settle for {}: {:#?}",
                    $stage, last_health
                )
            });
        }};
    }

    let mut gateway_request_ids = HashSet::new();
    let mut user_input_threads = HashSet::new();
    let mut permissions_threads = HashSet::new();
    let mut mcp_threads = HashSet::new();
    let mut refresh_accounts = HashSet::new();
    let mut user_input_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while user_input_requests.len() < expected_threads.len() {
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
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-server-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local requests are pending"
                        );
                        user_input_requests.push((request_id, params.thread_id));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent user-input server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "user-input",
        2,
        0,
        0,
        vec![GatewayV2ServerRequestBacklogMethodCounts {
            method: "item/tool/requestUserInput".to_string(),
            pending_server_request_count: 2,
            answered_but_unresolved_server_request_count: 0,
            server_request_backlog_count: 2,
        }]
    );
    for (request_id, thread_id) in user_input_requests {
        let answer = match thread_id.as_str() {
            "thread-worker-a" => "safe-a",
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
        user_input_threads.insert(thread_id);
    }

    let mut permission_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while permission_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, "turn-remote-workflow");
                        assert_eq!(params.item_id, "perm-remote-workflow");
                        assert_eq!(
                            params.reason,
                            Some("Need concurrent permissions".to_string())
                        );
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-permissions-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local permission requests are pending"
                        );
                        permission_requests.push((request_id, params));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent permission server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "permissions",
        4,
        2,
        1,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, params) in permission_requests {
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

    let mut mcp_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while mcp_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                        request_id,
                        params,
                    }) => {
                        assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                        assert_eq!(params.server_name, "mock-mcp");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-mcp-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local elicitation requests are pending"
                        );
                        mcp_requests.push((request_id, params));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent mcp elicitation server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "mcp elicitation",
        6,
        4,
        2,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, params) in mcp_requests {
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

    let mut refresh_requests = Vec::new();
    timeout(Duration::from_secs(5), async {
            while refresh_requests.len() < expected_threads.len() {
                let event = client
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                match event {
                    AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                        request_id,
                        params,
                    }) => {
                        let previous_account_id = params
                            .previous_account_id
                            .clone()
                            .expect("refresh request should include previous account id");
                        assert_ne!(
                            request_id,
                            RequestId::String("shared-concurrent-chatgpt-refresh-request".to_string())
                        );
                        assert!(
                            gateway_request_ids.insert(request_id.clone()),
                            "gateway request ids should stay unique while both worker-local refresh requests are pending"
                        );
                        refresh_requests.push((request_id, previous_account_id));
                    }
                    AppServerEvent::ServerNotification(
                        ServerNotification::ServerRequestResolved(_),
                    ) => {}
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        })
        .await
        .expect("concurrent ChatGPT token-refresh server requests should arrive from both workers");
    assert_concurrent_pending_server_request_health!(
        "ChatGPT token refresh",
        8,
        6,
        3,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 2,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    for (request_id, previous_account_id) in refresh_requests {
        client
            .resolve_server_request(
                request_id,
                serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                    access_token: format!("access-token-{previous_account_id}"),
                    chatgpt_account_id: previous_account_id.clone(),
                    chatgpt_plan_type: Some("pro".to_string()),
                })
                .expect("chatgpt refresh response should serialize"),
            )
            .await
            .expect("chatgpt refresh should resolve");
        refresh_accounts.insert(previous_account_id);
    }
    assert_eq!(user_input_threads, expected_threads);
    assert_eq!(permissions_threads, expected_threads);
    assert_eq!(mcp_threads, expected_threads);
    assert_eq!(
        refresh_accounts,
        HashSet::from([
            "account-thread-worker-a".to_string(),
            "account-thread-worker-b".to_string(),
        ])
    );
    assert_eq!(gateway_request_ids.len(), 8);

    assert_remote_client_shutdown(client.shutdown().await);
    let settled_health = timeout(Duration::from_secs(15), async {
        loop {
            let response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = response.json().await.expect("health body");
            if health.v2_connections.active_connection_count == 0
                && health
                    .v2_connections
                    .last_connection_server_request_backlog_method_counts
                    .len()
                    == 4
            {
                break health;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("last connection server-request backlog health should settle");
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_max_server_request_backlog_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_started_at
            .is_some(),
        true
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_pending_server_request_count,
        0
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_answered_but_unresolved_server_request_count,
        8
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_worker_counts,
        vec![
            GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(0),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_count: 4,
            },
            GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(1),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 4,
                server_request_backlog_count: 4,
            },
        ]
    );
    assert_eq!(
        settled_health
            .v2_connections
            .last_connection_server_request_backlog_method_counts,
        vec![
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
            GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 2,
            },
        ]
    );
    server.shutdown().await.expect("shutdown");
}
