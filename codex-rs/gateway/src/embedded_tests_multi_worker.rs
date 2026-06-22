use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_multi_worker_server_requests.rs"]
mod embedded_tests_multi_worker_server_requests;

#[tokio::test]
async fn remote_single_worker_supports_additional_server_request_roundtrips_over_v2() {
    let websocket_url = start_mock_remote_server_for_multiple_server_request_roundtrips().await;
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
    .expect("remote client should connect to remote gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
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
        .expect("thread/start should succeed through remote gateway");
    assert_eq!(started.thread.id, "thread-remote-workflow");

    let mut saw_command_request = false;
    let mut saw_file_request = false;
    let mut saw_mcp_elicitation_request = false;
    let mut saw_dynamic_tool_call = false;
    let mut saw_dynamic_tool_item_started = false;
    let mut saw_dynamic_tool_item_completed = false;
    let mut saw_permissions_request = false;
    let mut saw_refresh_request = false;
    let mut saw_turn_completed = false;
    let mut dynamic_tool_request_id: Option<RequestId> = None;

    timeout(Duration::from_secs(5), async {
        while !(saw_command_request
            && saw_file_request
            && saw_mcp_elicitation_request
            && saw_dynamic_tool_call
            && saw_dynamic_tool_item_started
            && saw_dynamic_tool_item_completed
            && saw_permissions_request
            && saw_refresh_request
            && saw_turn_completed)
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-remote-workflow");
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "cmd-remote-workflow");
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
                    saw_command_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-remote-workflow");
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "file-remote-workflow");
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
                    saw_file_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::McpServerElicitationRequest {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-remote-workflow");
                    assert_eq!(params.turn_id, Some("turn-remote-workflow".to_string()));
                    assert_eq!(params.server_name, "mock-mcp");
                    match params.request {
                        McpServerElicitationRequest::Form {
                            message,
                            requested_schema,
                            ..
                        } => {
                            assert_eq!(message, "Allow mock action?");
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
                        other => panic!("unexpected elicitation request: {other:?}"),
                    }
                    client
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
                        .expect("mcp elicitation should resolve");
                    saw_mcp_elicitation_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::DynamicToolCall {
                    request_id,
                    params,
                }) => {
                    dynamic_tool_request_id = Some(request_id.clone());
                    assert_eq!(params.thread_id, "thread-remote-workflow");
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.call_id, "tool-call-remote-workflow");
                    assert_eq!(params.tool, "image-edit");
                    assert_eq!(
                        params.arguments,
                        serde_json::json!({
                            "prompt": "Sharpen this image",
                            "strength": 0.5,
                        })
                    );
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(DynamicToolCallResponse {
                                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                                    text: "tool output".to_string(),
                                }],
                                success: true,
                            })
                            .expect("dynamic tool call response should serialize"),
                        )
                        .await
                        .expect("dynamic tool call should resolve");
                    saw_dynamic_tool_call = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == "thread-remote-workflow"
                    && notification.turn_id == "turn-remote-workflow"
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
                                } if id == "tool-call-remote-workflow"
                                    && tool == "image-edit"
                                    && *arguments
                                        == serde_json::json!({
                                            "prompt": "Sharpen this image",
                                            "strength": 0.5,
                                        })
                                    && *status == DynamicToolCallStatus::InProgress
                                    && content_items.is_none()
                                    && success.is_none()
                                    && duration_ms.is_none()
                            ) =>
                {
                    saw_dynamic_tool_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                    _,
                )) => {}
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == "thread-remote-workflow"
                    && notification.turn_id == "turn-remote-workflow"
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
                                } if id == "tool-call-remote-workflow"
                                    && tool == "image-edit"
                                    && *arguments
                                        == serde_json::json!({
                                            "prompt": "Sharpen this image",
                                            "strength": 0.5,
                                        })
                                    && *status == DynamicToolCallStatus::Completed
                                    && *content_items
                                        == Some(vec![
                                            DynamicToolCallOutputContentItem::InputText {
                                                text: "tool output".to_string(),
                                            },
                                        ])
                                    && *success == Some(true)
                                    && *duration_ms == Some(7)
                            ) =>
                {
                    saw_dynamic_tool_item_completed = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.thread_id, "thread-remote-workflow");
                    assert_eq!(params.turn_id, "turn-remote-workflow");
                    assert_eq!(params.item_id, "perm-remote-workflow");
                    assert_eq!(params.reason, Some("Need wider permissions".to_string()));
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
                    client
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
                        .expect("permissions approval should resolve");
                    saw_permissions_request = true;
                }
                AppServerEvent::ServerRequest(ServerRequest::ChatgptAuthTokensRefresh {
                    request_id,
                    params,
                }) => {
                    assert_eq!(params.previous_account_id, Some("acct-123".to_string()));
                    client
                        .resolve_server_request(
                            request_id,
                            serde_json::to_value(ChatgptAuthTokensRefreshResponse {
                                access_token: "access-token-1".to_string(),
                                chatgpt_account_id: "acct-123".to_string(),
                                chatgpt_plan_type: Some("pro".to_string()),
                            })
                            .expect("chatgpt refresh response should serialize"),
                        )
                        .await
                        .expect("chatgpt refresh should resolve");
                    saw_refresh_request = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    TurnCompletedNotification { thread_id, turn },
                )) => {
                    assert_eq!(thread_id, "thread-remote-workflow");
                    assert_eq!(turn.id, "turn-remote-workflow");
                    assert_eq!(turn.status, TurnStatus::Completed);
                    saw_turn_completed = true;
                }
                other => eprintln!("unexpected additional server request event: {other:?}"),
            }
        }
    })
    .await
    .expect("all additional server requests should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

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

#[tokio::test]
async fn remote_single_worker_forwards_connection_state_notifications_after_worker_reconnect() {
    let websocket_url =
        start_reconnecting_v2_single_worker_connection_state_notification_server().await;
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

    let mut saw_account_updated = false;
    let mut saw_rate_limits = false;
    let mut saw_app_list = false;
    let mut saw_login_completed = false;
    let mut saw_mcp_oauth_login_completed = false;
    let mut saw_mcp_startup_status = false;
    let mut saw_warning = false;
    let mut saw_config_warning = false;
    let mut saw_deprecation_notice = false;
    let mut saw_windows_world_writable_warning = false;
    let mut saw_windows_sandbox_setup_completed = false;
    timeout(Duration::from_secs(5), async {
        while !(saw_account_updated
            && saw_rate_limits
            && saw_app_list
            && saw_login_completed
            && saw_mcp_oauth_login_completed
            && saw_mcp_startup_status
            && saw_warning
            && saw_config_warning
            && saw_deprecation_notice
            && saw_windows_world_writable_warning
            && saw_windows_sandbox_setup_completed)
        {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after worker reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.auth_mode, None);
                    saw_account_updated = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::AccountRateLimitsUpdated(notification),
                ) => {
                    assert_eq!(notification.rate_limits.plan_type, None);
                    saw_rate_limits = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AppListUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.data.len(), 1);
                    assert_eq!(notification.data[0].name, "calendar");
                    saw_app_list = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                    notification,
                )) => {
                    assert_eq!(notification.login_id, None);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_login_completed = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::McpServerOauthLoginCompleted(notification),
                ) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_mcp_oauth_login_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::McpServerStatusUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.status, McpServerStartupState::Ready);
                    assert_eq!(notification.error, None);
                    saw_mcp_startup_status = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::Warning(notification)) => {
                    assert_eq!(notification.thread_id, None);
                    assert_eq!(notification.message, "shared warning");
                    saw_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ConfigWarning(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared config warning");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("check your shared config")
                    );
                    saw_config_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::DeprecationNotice(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared deprecation notice");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("update the shared workflow")
                    );
                    saw_deprecation_notice = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsWorldWritableWarning(notification),
                ) => {
                    assert_eq!(notification.sample_paths, vec!["C:\\shared-temp"]);
                    assert_eq!(notification.extra_count, 2);
                    assert_eq!(notification.failed_scan, false);
                    saw_windows_world_writable_warning = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsSandboxSetupCompleted(notification),
                ) => {
                    assert_eq!(notification.mode, WindowsSandboxSetupMode::Unelevated);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_windows_sandbox_setup_completed = true;
                }
                other => panic!("unexpected notification after worker reconnect: {other:?}"),
            }
        }
    })
    .await
    .expect("connection-state notifications should arrive after reconnect");

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_forwards_skills_changed_notifications_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_single_worker_skills_changed_server().await;
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
        channel_capacity: 16,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let first_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("initial skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        first_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));

    let _: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(1),
            params: SkillsListParams {
                cwds: vec!["/tmp/shared-repo".into()],
                force_reload: true,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after reconnect")
    .expect("skills/list should succeed through remote gateway after reconnect");

    let second_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_v2_clients_recover_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_thread_server(
        "thread-worker-a-2",
        "/tmp/worker-a-2",
    )
    .await;
    let worker_b =
        start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b").await;
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
                        websocket_url: worker_a.clone(),

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

    let client = reqwest::Client::new();
    let mut events_response = client
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
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
                && !remote_workers[1].reconnecting
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");
    timeout(Duration::from_secs(5), async {
        let mut buffered = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            buffered.push_str(std::str::from_utf8(&chunk).expect("utf8"));

            while let Some(event_end) = buffered.find("\n\n") {
                let event = buffered[..event_end].to_string();
                buffered.drain(..event_end + 2);
                if event.contains("event: gateway/reconnected") {
                    assert!(event.contains("\"workerId\":"));
                    assert!(event.contains(&worker_a));
                    return;
                }
            }
        }
    })
    .await
    .expect("reconnected SSE event should arrive before v2 client connects");
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
        channel_capacity: 8,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let first_started: AppServerThreadStartResponse = v2_client
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
        .expect("first thread/start should succeed after worker reconnect");
    assert_eq!(first_started.thread.id, "thread-worker-a-2");

    let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput { request_id, params }) =
        timeout(Duration::from_secs(5), v2_client.next_event())
            .await
            .expect("server request should arrive after multi-worker reconnect")
            .expect("event stream should stay open after multi-worker reconnect")
    else {
        panic!("expected tool/requestUserInput after multi-worker reconnect");
    };
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
    v2_client
        .resolve_server_request(
            request_id,
            serde_json::to_value(ToolRequestUserInputResponse { answers })
                .expect("server request response should serialize"),
        )
        .await
        .expect("server request should resolve after multi-worker reconnect");

    let recovered_turn_started: TurnStartResponse = v2_client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "hello recovered worker".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start should succeed after multi-worker reconnect");
    assert_eq!(recovered_turn_started.turn.id, "turn-thread-worker-a-2");
    assert_eq!(recovered_turn_started.turn.status, TurnStatus::InProgress);

    let mut recovered_turn_coverage = TurnStreamingCoverage::default();
    let recovered_turn_result = timeout(Duration::from_secs(5), async {
        while recovered_turn_coverage
            != (TurnStreamingCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_hook_started: true,
                saw_item_started: true,
                saw_agent_delta: true,
                saw_reasoning_summary_delta: true,
                saw_reasoning_text_delta: true,
                saw_command_output_delta: true,
                saw_file_change_delta: true,
                saw_hook_completed: true,
                saw_item_completed: true,
                saw_turn_completed: true,
            })
        {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after multi-worker reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && matches!(
                        notification.status,
                        ThreadStatus::Active { ref active_flags } if active_flags.is_empty()
                    ) =>
                {
                    recovered_turn_coverage.saw_thread_active = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == "turn-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-thread-worker-a-2")
                    && notification.run.id == "hook-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_hook_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-thread-worker-a-2" && text == "streaming answer in progress"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "hello from recovered worker" =>
                {
                    recovered_turn_coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "summary thread-worker-a-2"
                    && notification.summary_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "reasoning thread-worker-a-2"
                    && notification.content_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "stdout thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_command_output_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && notification.delta == "patch thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_file_change_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some("turn-thread-worker-a-2")
                    && notification.run.id == "hook-thread-worker-a-2" =>
                {
                    recovered_turn_coverage.saw_hook_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == "turn-thread-worker-a-2"
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == "msg-thread-worker-a-2" && text == "streaming answer completed"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == "turn-thread-worker-a-2"
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    recovered_turn_coverage.saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        recovered_turn_result.is_ok(),
        true,
        "turn notifications should fan in after multi-worker reconnect: {recovered_turn_coverage:?}"
    );

    let second_started: AppServerThreadStartResponse = v2_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
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
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let listed: AppServerThreadListResponse = v2_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(4),
            params: ThreadListParams {
                parent_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        })
        .await
        .expect("thread/list should succeed after worker reconnect");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a-2"]
    );

    let first_read: AppServerThreadReadResponse = v2_client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to recovered worker");
    assert_eq!(first_read.thread.id, "thread-worker-a-2");
    assert_eq!(first_read.thread.preview, "/tmp/worker-a-2");

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_bootstrap_refresh_requests_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_bootstrap_refresh_server().await;
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
        channel_capacity: 8,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time after worker reconnect")
    .expect("account/read should succeed after worker reconnect");
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::Chatgpt {
                email: "gateway@example.com".to_string(),
                plan_type: AccountPlanType::Pro,
            }),
            requires_openai_auth: false,
        }
    );

    let codex_rate_limit = RateLimitSnapshot {
        limit_id: Some("codex".to_string()),
        limit_name: Some("Codex".to_string()),
        primary: Some(RateLimitWindow {
            used_percent: 42,
            window_duration_mins: Some(300),
            resets_at: Some(1_700_000_000),
        }),
        secondary: None,
        credits: None,
        plan_type: Some(AccountPlanType::Pro),
        rate_limit_reached_type: None,
        individual_limit: None,
    };
    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(2),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time after worker reconnect")
    .expect("account/rateLimits/read should succeed after worker reconnect");
    assert_eq!(
        rate_limits,
        GetAccountRateLimitsResponse {
            rate_limits: codex_rate_limit.clone(),
            rate_limits_by_limit_id: Some(
                [("codex".to_string(), codex_rate_limit)]
                    .into_iter()
                    .collect(),
            ),
            rate_limit_reset_credits: None,
        }
    );

    let models: ModelListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(3),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list should finish in time after worker reconnect")
    .expect("model/list should succeed after worker reconnect");
    assert_eq!(models.next_cursor, None);
    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "openai/gpt-5");

    let external_agent_config: ExternalAgentConfigDetectResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExternalAgentConfigDetect {
            request_id: RequestId::Integer(4),
            params: ExternalAgentConfigDetectParams {
                include_home: true,
                cwds: Some(vec![PathBuf::from("/tmp/reconnected-project")]),
            },
        }),
    )
    .await
    .expect("externalAgentConfig/detect should finish in time after worker reconnect")
    .expect("externalAgentConfig/detect should succeed after worker reconnect");
    assert_eq!(external_agent_config.items.len(), 1);
    assert_eq!(
        external_agent_config.items[0].description,
        "reconnected repo config"
    );

    let apps: AppsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(5),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: None,
                force_refetch: false,
            },
        }),
    )
    .await
    .expect("app/list should finish in time after worker reconnect")
    .expect("app/list should succeed after worker reconnect");
    assert_eq!(apps.next_cursor, None);
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "reconnected-app");

    let skills: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(6),
            params: SkillsListParams {
                cwds: vec![PathBuf::from("/tmp/reconnected-project")],
                force_reload: false,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after worker reconnect")
    .expect("skills/list should succeed after worker reconnect");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(
        skills.data[0].cwd,
        PathBuf::from("/tmp/reconnected-project")
    );
    assert_eq!(skills.data[0].skills.len(), 1);
    assert_eq!(skills.data[0].skills[0].name, "reconnected-skill");

    let plugin_list_params: PluginListParams = serde_json::from_value(serde_json::json!({
        "cwds": ["/tmp/reconnected-project"],
    }))
    .expect("plugin/list params should deserialize");
    let plugins: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(7),
            params: plugin_list_params,
        }),
    )
    .await
    .expect("plugin/list should finish in time after worker reconnect")
    .expect("plugin/list should succeed after worker reconnect");
    assert_eq!(plugins.marketplace_load_errors, Vec::new());
    assert_eq!(plugins.marketplaces.len(), 1);
    assert_eq!(plugins.marketplaces[0].name, "reconnected-marketplace");
    assert_eq!(plugins.marketplaces[0].plugins.len(), 1);
    assert_eq!(plugins.marketplaces[0].plugins[0].id, "reconnected-plugin");
    assert_eq!(
        plugins.featured_plugin_ids,
        vec!["reconnected-plugin".to_string()]
    );

    let plugin: PluginReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginRead {
            request_id: RequestId::Integer(8),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/reconnected-project/marketplace.json",
                "pluginName": "reconnected-plugin",
            }))
            .expect("plugin/read params should deserialize"),
        }),
    )
    .await
    .expect("plugin/read should finish in time after worker reconnect")
    .expect("plugin/read should succeed after worker reconnect");
    assert_eq!(plugin.plugin.summary.name, "reconnected-plugin");
    assert_eq!(plugin.plugin.marketplace_name, "reconnected-marketplace");
    assert_eq!(plugin.plugin.skills.len(), 1);
    assert_eq!(plugin.plugin.skills[0].name, "reconnected-skill");

    let install: PluginInstallResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginInstall {
            request_id: RequestId::Integer(9),
            params: serde_json::from_value(serde_json::json!({
                "marketplacePath": "/tmp/reconnected-project/marketplace.json",
                "pluginName": "reconnected-plugin",
            }))
            .expect("plugin/install params should deserialize"),
        }),
    )
    .await
    .expect("plugin/install should finish in time after worker reconnect")
    .expect("plugin/install should succeed after worker reconnect");
    assert_eq!(
        install,
        PluginInstallResponse {
            auth_policy: PluginAuthPolicy::OnInstall,
            apps_needing_auth: Vec::new(),
        }
    );

    let plugins_after_install: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(10),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/reconnected-project"],
            }))
            .expect("plugin/list params after install should deserialize"),
        }),
    )
    .await
    .expect("plugin/list after install should finish in time after worker reconnect")
    .expect("plugin/list after install should succeed after worker reconnect");
    assert_eq!(
        plugins_after_install.marketplaces[0].plugins[0].installed,
        true
    );

    let uninstall: PluginUninstallResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginUninstall {
            request_id: RequestId::Integer(11),
            params: PluginUninstallParams {
                plugin_id: "reconnected-plugin@reconnected-marketplace".to_string(),
            },
        }),
    )
    .await
    .expect("plugin/uninstall should finish in time after worker reconnect")
    .expect("plugin/uninstall should succeed after worker reconnect");
    assert_eq!(uninstall, PluginUninstallResponse {});

    let plugins_after_uninstall: PluginListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::PluginList {
            request_id: RequestId::Integer(12),
            params: serde_json::from_value(serde_json::json!({
                "cwds": ["/tmp/reconnected-project"],
            }))
            .expect("plugin/list params after uninstall should deserialize"),
        }),
    )
    .await
    .expect("plugin/list after uninstall should finish in time after worker reconnect")
    .expect("plugin/list after uninstall should succeed after worker reconnect");
    assert_eq!(
        plugins_after_uninstall.marketplaces[0].plugins[0].installed,
        false
    );

    let config_read: ConfigReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigRead {
            request_id: RequestId::Integer(13),
            params: ConfigReadParams {
                include_layers: true,
                cwd: Some("/tmp/reconnected-project".to_string()),
            },
        }),
    )
    .await
    .expect("config/read should finish in time after worker reconnect")
    .expect("config/read should succeed after worker reconnect");
    assert_eq!(config_read.config.model.as_deref(), Some("gpt-5"));
    assert_eq!(config_read.layers.is_some(), true);

    let config_requirements: ConfigRequirementsReadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(14),
            params: None,
        }),
    )
    .await
    .expect("configRequirements/read should finish in time after worker reconnect")
    .expect("configRequirements/read should succeed after worker reconnect");
    assert_eq!(config_requirements.requirements, None);

    let experimental_features: ExperimentalFeatureListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExperimentalFeatureList {
            request_id: RequestId::Integer(15),
            params: ExperimentalFeatureListParams {
                cursor: None,
                limit: Some(10),
                ..Default::default()
            },
        }),
    )
    .await
    .expect("experimentalFeature/list should finish in time after worker reconnect")
    .expect("experimentalFeature/list should succeed after worker reconnect");
    assert_eq!(experimental_features.next_cursor, None);
    assert_eq!(experimental_features.data.len(), 1);
    assert_eq!(experimental_features.data[0].name, "gateway-test-feature");

    let collaboration_modes: CollaborationModeListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CollaborationModeList {
            request_id: RequestId::Integer(16),
            params: CollaborationModeListParams::default(),
        }),
    )
    .await
    .expect("collaborationMode/list should finish in time after worker reconnect")
    .expect("collaborationMode/list should succeed after worker reconnect");
    assert_eq!(collaboration_modes.data.len(), 1);
    assert_eq!(collaboration_modes.data[0].name, "default");

    let mcp_statuses: ListMcpServerStatusResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerStatusList {
            request_id: RequestId::Integer(17),
            params: ListMcpServerStatusParams {
                cursor: None,
                limit: None,
                detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServerStatus/list should finish in time after worker reconnect")
    .expect("mcpServerStatus/list should succeed after worker reconnect");
    assert_eq!(mcp_statuses.data.len(), 1);
    assert_eq!(mcp_statuses.data[0].name, "reconnected-mcp");

    let imported: ExternalAgentConfigImportResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(18),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        }),
    )
    .await
    .expect("externalAgentConfig/import should finish in time after worker reconnect")
    .expect("externalAgentConfig/import should succeed after worker reconnect");
    assert!(!imported.import_id.is_empty());

    let marketplace: MarketplaceAddResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(19),
            params: MarketplaceAddParams {
                source: "https://example.com/reconnected-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/reconnected-plugin".to_string()]),
            },
        }),
    )
    .await
    .expect("marketplace/add should finish in time after worker reconnect")
    .expect("marketplace/add should succeed after worker reconnect");
    assert_eq!(marketplace.marketplace_name, "reconnected-marketplace");
    assert_eq!(marketplace.already_added, false);
    assert_eq!(
        marketplace.installed_root.as_path(),
        PathBuf::from("/tmp/reconnected-project/marketplace").as_path()
    );

    let skills_config: SkillsConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(20),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/reconnected-project/.codex/skills/reconnected-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        }),
    )
    .await
    .expect("skills/config/write should finish in time after worker reconnect")
    .expect("skills/config/write should succeed after worker reconnect");
    assert_eq!(skills_config.effective_enabled, true);

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(21),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        }),
    )
    .await
    .expect("experimentalFeature/enablement/set should finish in time after worker reconnect")
    .expect("experimentalFeature/enablement/set should succeed after worker reconnect");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let mcp_refresh: McpServerRefreshResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(22),
            params: None,
        }),
    )
    .await
    .expect("config/mcpServer/reload should finish in time after worker reconnect")
    .expect("config/mcpServer/reload should succeed after worker reconnect");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let batch_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(23),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/reconnected-project/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        }),
    )
    .await
    .expect("config/batchWrite should finish in time after worker reconnect")
    .expect("config/batchWrite should succeed after worker reconnect");
    assert_eq!(batch_write.status, WriteStatus::Ok);
    assert_eq!(batch_write.version, "reconnected-version-1");
    assert_eq!(
        batch_write.file_path.as_path(),
        PathBuf::from("/tmp/reconnected-project/config.toml").as_path()
    );
    assert_eq!(batch_write.overridden_metadata, None);

    let config_value_write: ConfigWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(24),
            params: ConfigValueWriteParams {
                key_path: "plugins.reconnected-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        }),
    )
    .await
    .expect("config/value/write should finish in time after worker reconnect")
    .expect("config/value/write should succeed after worker reconnect");
    assert_eq!(config_value_write.status, WriteStatus::Ok);
    assert_eq!(config_value_write.version, "reconnected-version-1");
    assert_eq!(
        config_value_write.file_path.as_path(),
        PathBuf::from("/tmp/reconnected-project/config.toml").as_path()
    );
    assert_eq!(config_value_write.overridden_metadata, None);

    let reset: MemoryResetResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(25),
            params: None,
        }),
    )
    .await
    .expect("memory/reset should finish in time after worker reconnect")
    .expect("memory/reset should succeed after worker reconnect");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(26),
            params: None,
        }),
    )
    .await
    .expect("account/logout should finish in time after worker reconnect")
    .expect("account/logout should succeed after worker reconnect");
    assert_eq!(logout, LogoutAccountResponse {});

    let feedback: FeedbackUploadResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(27),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway single-worker recovery".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        }),
    )
    .await
    .expect("feedback/upload should finish in time after worker reconnect")
    .expect("feedback/upload should succeed after worker reconnect");
    assert_eq!(feedback.thread_id, "feedback-thread-reconnected");

    let add_credits: SendAddCreditsNudgeEmailResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(28),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        }),
    )
    .await
    .expect("account/sendAddCreditsNudgeEmail should finish in time after worker reconnect")
    .expect("account/sendAddCreditsNudgeEmail should succeed after worker reconnect");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let create_directory: FsCreateDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(29),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        }),
    )
    .await
    .expect("fs/createDirectory should finish in time after worker reconnect")
    .expect("fs/createDirectory should succeed after worker reconnect");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(30),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVjb25uZWN0ZWQtZmlsZQ==".to_string(),
            },
        }),
    )
    .await
    .expect("fs/writeFile should finish in time after worker reconnect")
    .expect("fs/writeFile should succeed after worker reconnect");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(31),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readFile should finish in time after worker reconnect")
    .expect("fs/readFile should succeed after worker reconnect");
    assert_eq!(read_file.data_base64, "cmVjb25uZWN0ZWQtZmlsZQ==");

    let metadata: FsGetMetadataResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(32),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/getMetadata should finish in time after worker reconnect")
    .expect("fs/getMetadata should succeed after worker reconnect");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(33),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readDirectory should finish in time after worker reconnect")
    .expect("fs/readDirectory should succeed after worker reconnect");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");

    let copy: FsCopyResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(34),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        }),
    )
    .await
    .expect("fs/copy should finish in time after worker reconnect")
    .expect("fs/copy should succeed after worker reconnect");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(35),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(false),
                force: Some(true),
            },
        }),
    )
    .await
    .expect("fs/remove should finish in time after worker reconnect")
    .expect("fs/remove should succeed after worker reconnect");
    assert_eq!(remove, FsRemoveResponse {});

    let watch: FsWatchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(36),
            params: FsWatchParams {
                watch_id: "reconnected-watch".to_string(),
                path: PathBuf::from("/tmp/reconnected-project/config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/watch should finish in time after worker reconnect")
    .expect("fs/watch should succeed after worker reconnect");
    assert_eq!(
        watch.path.as_ref().to_string_lossy(),
        "/tmp/reconnected-project/config.toml"
    );

    let fs_changed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed should arrive after recovered fs/watch");
    assert_eq!(fs_changed.watch_id, "reconnected-watch");
    assert_eq!(
        fs_changed
            .changed_paths
            .iter()
            .map(|path| path.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec!["/tmp/reconnected-project/config.toml".to_string()]
    );

    let fuzzy_search: FuzzyFileSearchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(37),
            params: FuzzyFileSearchParams {
                query: "reconnect".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
                cancellation_token: Some("reconnected-fuzzy-search".to_string()),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch should finish in time after worker reconnect")
    .expect("fuzzyFileSearch should succeed after worker reconnect");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].root, "/tmp/reconnected-project");
    assert_eq!(fuzzy_search.files[0].path, "docs/reconnected.md");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(38),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStart should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStart should succeed after worker reconnect");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(39),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                query: "reconnect".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionUpdate should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionUpdate should succeed after worker reconnect");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy update");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
                && notification.query == "reconnect"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated should arrive after worker reconnect");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(
        fuzzy_session_updated.files[0].root,
        "/tmp/reconnected-project"
    );
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/reconnected.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(40),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "reconnected-fuzzy-session".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStop should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStop should succeed after worker reconnect");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy stop");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted should arrive after worker reconnect");
    assert_eq!(
        fuzzy_session_completed.session_id,
        "reconnected-fuzzy-session"
    );

    let windows_setup: WindowsSandboxSetupStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(41),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/reconnected-project")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        }),
    )
    .await
    .expect("windowsSandbox/setupStart should finish in time after worker reconnect")
    .expect("windowsSandbox/setupStart should succeed after worker reconnect");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered windows setup");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted should arrive after worker reconnect");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(42),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf remote-command".to_string(),
                ],
                process_id: Some("proc-remote".to_string()),
                tty: true,
                stream_stdin: true,
                stream_stdout_stderr: true,
                output_bytes_cap: None,
                disable_output_cap: false,
                disable_timeout: false,
                timeout_ms: None,
                cwd: None,
                env: None,
                size: Some(CommandExecTerminalSize { rows: 24, cols: 80 }),
                sandbox_policy: None,
                permission_profile: None,
            },
        }),
    )
    .await
    .expect("command/exec should finish in time after worker reconnect")
    .expect("command/exec should succeed after worker reconnect");
    assert_eq!(
        command_exec,
        CommandExecResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    );

    let command_output = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered command/exec");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
                && notification.process_id == "proc-remote"
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta should arrive after worker reconnect");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "cmVtb3RlLWNvbW1hbmQ=");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(43),
            params: CommandExecWriteParams {
                process_id: "proc-remote".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        }),
    )
    .await
    .expect("command/exec/write should finish in time after worker reconnect")
    .expect("command/exec/write should succeed after worker reconnect");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(44),
            params: CommandExecResizeParams {
                process_id: "proc-remote".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        }),
    )
    .await
    .expect("command/exec/resize should finish in time after worker reconnect")
    .expect("command/exec/resize should succeed after worker reconnect");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(45),
            params: CommandExecTerminateParams {
                process_id: "proc-remote".to_string(),
            },
        }),
    )
    .await
    .expect("command/exec/terminate should finish in time after worker reconnect")
    .expect("command/exec/terminate should succeed after worker reconnect");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    let mcp_oauth_login: McpServerOauthLoginResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(46),
            params: McpServerOauthLoginParams {
                name: "reconnected-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
            },
        }),
    )
    .await
    .expect("mcpServer/oauth/login should finish in time after worker reconnect")
    .expect("mcpServer/oauth/login should succeed after worker reconnect");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/reconnected-mcp"
    );

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::McpServerOauthLoginCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/oauthLogin/completed should arrive after worker reconnect");
    assert_eq!(mcp_oauth_completed.name, "reconnected-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), v2_client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_supports_v2_realtime_workflow_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_realtime_server_with_voices(
        "thread-worker-a-2",
        "/tmp/worker-a-2",
        "session-worker-a-2",
        "delta from recovered worker a",
        "done from recovered worker a",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Juniper, RealtimeVoice::Maple],
            v2: vec![RealtimeVoice::Alloy],
            default_v1: RealtimeVoice::Juniper,
            default_v2: RealtimeVoice::Alloy,
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_realtime_server_with_voices(
        "thread-worker-b",
        "/tmp/worker-b",
        "session-worker-b",
        "delta from worker b",
        "done from worker b",
        RealtimeVoicesList {
            v1: vec![RealtimeVoice::Maple, RealtimeVoice::Cove],
            v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
            default_v1: RealtimeVoice::Cove,
            default_v2: RealtimeVoice::Marin,
        },
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

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
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
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
                && !remote_workers[1].reconnecting
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
            request_id: RequestId::Integer(0),
            params: ThreadRealtimeListVoicesParams {},
        })
        .await
        .expect("thread/realtime/listVoices should succeed after worker reconnect");
    assert_eq!(
        voices,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList {
                v1: vec![
                    RealtimeVoice::Juniper,
                    RealtimeVoice::Maple,
                    RealtimeVoice::Cove,
                ],
                v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
                default_v1: RealtimeVoice::Juniper,
                default_v2: RealtimeVoice::Alloy,
            },
        }
    );

    let first_started: AppServerThreadStartResponse = v2_client
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
        .expect("first thread/start should succeed after worker reconnect");
    assert_eq!(first_started.thread.id, "thread-worker-a-2");

    let second_started: AppServerThreadStartResponse = v2_client
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
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let first_realtime_started: ThreadRealtimeStartResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(3),
            params: ThreadRealtimeStartParams {
                thread_id: first_started.thread.id.clone(),
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
        .expect("first realtime start should succeed after worker reconnect");
    assert_eq!(first_realtime_started, ThreadRealtimeStartResponse {});

    let second_realtime_started: ThreadRealtimeStartResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(4),
            params: ThreadRealtimeStartParams {
                thread_id: second_started.thread.id.clone(),
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
        .expect("second realtime start should succeed after worker reconnect");
    assert_eq!(second_realtime_started, ThreadRealtimeStartResponse {});

    let first_append_text: ThreadRealtimeAppendTextResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(5),
            params: ThreadRealtimeAppendTextParams {
                thread_id: first_started.thread.id.clone(),
                text: "hello recovered realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("first realtime appendText should succeed after worker reconnect");
    assert_eq!(first_append_text, ThreadRealtimeAppendTextResponse {});

    let second_append_text: ThreadRealtimeAppendTextResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(6),
            params: ThreadRealtimeAppendTextParams {
                thread_id: second_started.thread.id.clone(),
                text: "hello worker b realtime".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("second realtime appendText should succeed after worker reconnect");
    assert_eq!(second_append_text, ThreadRealtimeAppendTextResponse {});

    let first_append_audio: ThreadRealtimeAppendAudioResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(7),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: first_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-a".to_string()),
                },
            },
        })
        .await
        .expect("first realtime appendAudio should succeed after worker reconnect");
    assert_eq!(first_append_audio, ThreadRealtimeAppendAudioResponse {});

    let second_append_audio: ThreadRealtimeAppendAudioResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(8),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: second_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "BAUG".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("item-audio-b".to_string()),
                },
            },
        })
        .await
        .expect("second realtime appendAudio should succeed after worker reconnect");
    assert_eq!(second_append_audio, ThreadRealtimeAppendAudioResponse {});

    let first_stop: ThreadRealtimeStopResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(9),
            params: ThreadRealtimeStopParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first realtime stop should succeed after worker reconnect");
    assert_eq!(first_stop, ThreadRealtimeStopResponse {});

    let second_stop: ThreadRealtimeStopResponse = v2_client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(10),
            params: ThreadRealtimeStopParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second realtime stop should succeed after worker reconnect");
    assert_eq!(second_stop, ThreadRealtimeStopResponse {});

    let mut coverage_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            RealtimeStreamingCoverage::default(),
        ),
    ]);
    let expected_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            (
                "session-worker-a-2",
                "delta from recovered worker a",
                "done from recovered worker a",
                "AQID",
            ),
        ),
        (
            second_started.thread.id.clone(),
            (
                "session-worker-b",
                "delta from worker b",
                "done from worker b",
                "BAUG",
            ),
        ),
    ]);

    timeout(Duration::from_secs(5), async {
        while coverage_by_thread.values().any(|coverage| {
            *coverage
                != RealtimeStreamingCoverage {
                    saw_started: true,
                    saw_item_added: true,
                    saw_output_audio_delta: true,
                    saw_transcript_delta: true,
                    saw_transcript_done: true,
                    saw_sdp: true,
                    saw_error: true,
                    saw_closed: true,
                }
        }) {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) => {
                    if let Some((expected_session_id, _, _, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.realtime_session_id.as_deref() == Some(*expected_session_id)
                        && notification.version == RealtimeConversationVersion::V2
                    {
                        coverage.saw_started = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.item["type"] == "message"
                    {
                        coverage.saw_item_added = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) => {
                    if let Some((_, expected_delta, _, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.role == "assistant"
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_transcript_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) => {
                    if let Some((_, _, expected_text, _)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.role == "assistant"
                        && notification.text == *expected_text
                    {
                        coverage.saw_transcript_done = true;
                    }
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) => {
                    if let Some((_, _, _, expected_audio)) =
                        expected_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.audio.data == *expected_audio
                        && notification.audio.sample_rate == 24_000
                        && notification.audio.num_channels == 1
                    {
                        coverage.saw_output_audio_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.sdp.contains("s=Codex")
                    {
                        coverage.saw_sdp = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.message == "realtime transport warning"
                    {
                        coverage.saw_error = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.reason.as_deref() == Some("client requested stop")
                    {
                        coverage.saw_closed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime notifications should fan in after reconnect");

    assert_eq!(
        coverage_by_thread.get(&first_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );
    assert_eq!(
        coverage_by_thread.get(&second_started.thread.id),
        Some(&RealtimeStreamingCoverage {
            saw_started: true,
            saw_item_added: true,
            saw_output_audio_delta: true,
            saw_transcript_delta: true,
            saw_transcript_done: true,
            saw_sdp: true,
            saw_error: true,
            saw_closed: true,
        })
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_deduplicates_connection_state_notifications_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_state_notification_server().await;
    let worker_b = start_mock_remote_multi_connection_state_notification_server().await;
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

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
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
                && !remote_workers[0].reconnecting
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
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
        channel_capacity: 16,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let mut saw_account_updated = false;
    let mut saw_rate_limits = false;
    let mut saw_app_list = false;
    let mut saw_login_completed = false;
    let mut saw_mcp_oauth_login_completed = false;
    let mut saw_mcp_startup = false;
    let mut saw_warning = false;
    let mut saw_config_warning = false;
    let mut saw_deprecation_notice = false;
    let mut saw_windows_world_writable_warning = false;
    let mut saw_windows_sandbox_setup_completed = false;
    timeout(Duration::from_secs(5), async {
        while !(saw_account_updated
            && saw_rate_limits
            && saw_app_list
            && saw_login_completed
            && saw_mcp_oauth_login_completed
            && saw_mcp_startup
            && saw_warning
            && saw_config_warning
            && saw_deprecation_notice
            && saw_windows_world_writable_warning
            && saw_windows_sandbox_setup_completed)
        {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after worker reconnect");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.auth_mode, None);
                    saw_account_updated = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::AccountRateLimitsUpdated(notification),
                ) => {
                    assert_eq!(notification.rate_limits.plan_type, None);
                    saw_rate_limits = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AppListUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.data.len(), 1);
                    assert_eq!(notification.data[0].name, "calendar");
                    saw_app_list = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                    notification,
                )) => {
                    assert_eq!(notification.login_id, None);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_login_completed = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::McpServerOauthLoginCompleted(notification),
                ) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_mcp_oauth_login_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::McpServerStatusUpdated(
                    notification,
                )) => {
                    assert_eq!(notification.name, "calendar-mcp");
                    assert_eq!(notification.status, McpServerStartupState::Ready);
                    assert_eq!(notification.error, None);
                    saw_mcp_startup = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::Warning(notification)) => {
                    assert_eq!(notification.thread_id, None);
                    assert_eq!(notification.message, "shared warning");
                    saw_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ConfigWarning(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared config warning");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("check your shared config")
                    );
                    saw_config_warning = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::DeprecationNotice(
                    notification,
                )) => {
                    assert_eq!(notification.summary, "shared deprecation notice");
                    assert_eq!(
                        notification.details.as_deref(),
                        Some("update the shared workflow")
                    );
                    saw_deprecation_notice = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsWorldWritableWarning(notification),
                ) => {
                    assert_eq!(notification.sample_paths, vec!["C:\\shared-temp"]);
                    assert_eq!(notification.extra_count, 2);
                    assert_eq!(notification.failed_scan, false);
                    saw_windows_world_writable_warning = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::WindowsSandboxSetupCompleted(notification),
                ) => {
                    assert_eq!(notification.mode, WindowsSandboxSetupMode::Unelevated);
                    assert_eq!(notification.success, true);
                    assert_eq!(notification.error, None);
                    saw_windows_sandbox_setup_completed = true;
                }
                other => panic!("unexpected notification after worker reconnect: {other:?}"),
            }
        }
    })
    .await
    .expect("deduplicated state notifications should arrive after reconnect");
    assert!(saw_account_updated);
    assert!(saw_rate_limits);
    assert!(saw_app_list);
    assert!(saw_login_completed);
    assert!(saw_mcp_oauth_login_completed);
    assert!(saw_mcp_startup);
    assert!(saw_warning);
    assert!(saw_config_warning);
    assert!(saw_deprecation_notice);
    assert!(saw_windows_world_writable_warning);
    assert!(saw_windows_sandbox_setup_completed);
    assert!(
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate connection-state notifications should still be suppressed after reconnect"
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_deduplicates_skills_changed_notifications_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_skills_changed_server().await;
    let worker_b = start_mock_remote_multi_connection_skills_changed_server().await;
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

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
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
        channel_capacity: 8,
    })
    .await
    .expect("v2 client should connect after worker reconnect");

    let first_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("initial skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        first_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate initial skills/changed should be suppressed after reconnect"
    );

    let _: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(1),
            params: SkillsListParams {
                cwds: vec!["/tmp/shared-repo".into()],
                force_reload: true,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after reconnect")
    .expect("skills/list should succeed through multi-worker gateway after reconnect");

    let second_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate post-refresh skills/changed should be suppressed after reconnect"
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[path = "embedded_tests_multi_worker_late.rs"]
mod embedded_tests_multi_worker_late;
