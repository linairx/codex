use super::*;
use pretty_assertions::assert_eq;

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
                                == Some(vec![DynamicToolCallOutputContentItem::InputText {
                                    text: "tool output".to_string(),
                                }])
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
