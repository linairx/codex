use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_client_thread_routing_and_aggregation() {
    let worker_a =
        start_mock_remote_multi_connection_thread_server("thread-worker-a", "/tmp/worker-a").await;
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
        .expect("first thread/start should succeed through multi-worker remote gateway");
    assert_eq!(first_started.thread.id, "thread-worker-a");

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
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(3),
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
        .expect("thread/list should succeed through multi-worker remote gateway");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a"]
    );

    let loaded: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(4),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should succeed through multi-worker remote gateway");
    assert_eq!(loaded.next_cursor, None);
    assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);

    let first_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to worker A");
    assert_eq!(first_read.thread.id, first_started.thread.id);
    assert_eq!(
        first_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );

    let second_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: second_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to worker B");
    assert_eq!(second_read.thread.id, second_started.thread.id);
    assert_eq!(
        second_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b"
    );

    let first_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(7),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(first_started.thread.id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to worker A");
    assert_eq!(first_apps.next_cursor, None);
    assert_eq!(
        first_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-app"]
    );

    let second_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(8),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(second_started.thread.id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to worker B");
    assert_eq!(second_apps.next_cursor, None);
    assert_eq!(
        second_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b-app"]
    );

    let first_mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(9),
            params: McpResourceReadParams {
                thread_id: Some(first_started.thread.id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-a/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to worker A");
    assert_eq!(
        serde_json::to_value(&first_mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-a/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-a resource",
        }])
    );

    let second_mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(10),
            params: McpResourceReadParams {
                thread_id: Some(second_started.thread.id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-b/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to worker B");
    assert_eq!(
        serde_json::to_value(&second_mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-b/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-b resource",
        }])
    );

    let first_mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(11),
            params: McpServerToolCallParams {
                thread_id: first_started.thread.id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-a" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to worker A");
    assert_eq!(
        first_mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-a" }))
    );
    assert_eq!(first_mcp_tool.is_error, Some(false));

    let second_mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(12),
            params: McpServerToolCallParams {
                thread_id: second_started.thread.id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-b" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to worker B");
    assert_eq!(
        second_mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-b" }))
    );
    assert_eq!(second_mcp_tool.is_error, Some(false));

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
