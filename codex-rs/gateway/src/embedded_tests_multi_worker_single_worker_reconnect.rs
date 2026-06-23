use super::*;
use pretty_assertions::assert_eq;

#[path = "embedded_tests_multi_worker_single_worker_bootstrap.rs"]
mod embedded_tests_multi_worker_single_worker_bootstrap;
#[path = "embedded_tests_multi_worker_single_worker_reconnect_realtime.rs"]
mod embedded_tests_multi_worker_single_worker_reconnect_realtime;
#[path = "embedded_tests_multi_worker_single_worker_reconnect_thread_control.rs"]
mod embedded_tests_multi_worker_single_worker_reconnect_thread_control;

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
