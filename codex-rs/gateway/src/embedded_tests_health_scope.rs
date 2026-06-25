use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_scope_filters_threads_across_tenants_and_projects() {
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
                        account_id: Some("acct-a".to_string()),
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: Some("acct-b".to_string()),
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

    let owner_client = RemoteAppServerClient::connect_with_headers(
        RemoteAppServerConnectArgs {
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
        },
        vec![
            ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
            ("x-codex-project-id".to_string(), "project-a".to_string()),
        ],
    )
    .await
    .expect("owner client should connect to multi-worker gateway");

    let first_started: AppServerThreadStartResponse = owner_client
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

    let same_scope_client = RemoteAppServerClient::connect_with_headers(
        RemoteAppServerConnectArgs {
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
        },
        vec![
            ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
            ("x-codex-project-id".to_string(), "project-a".to_string()),
        ],
    )
    .await
    .expect("same-scope client should connect to multi-worker gateway");

    let same_scope_list: AppServerThreadListResponse = same_scope_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(3),
            params: ThreadListParams {
                parent_thread_id: None,
                ancestor_thread_id: None,
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
        .expect("same-scope thread/list should succeed through multi-worker remote gateway");
    assert_eq!(same_scope_list.next_cursor, None);
    assert_eq!(
        same_scope_list
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-a"]
    );

    let same_scope_loaded: ThreadLoadedListResponse = same_scope_client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(4),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("same-scope thread/loaded/list should succeed through multi-worker gateway");
    assert_eq!(same_scope_loaded.next_cursor, None);
    assert_eq!(same_scope_loaded.data, vec!["thread-worker-a"]);

    let same_scope_read: AppServerThreadReadResponse = same_scope_client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(5),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("same-scope thread/read should succeed through multi-worker gateway");
    assert_eq!(same_scope_read.thread.id, first_started.thread.id);

    let other_project_client = RemoteAppServerClient::connect_with_headers(
        RemoteAppServerConnectArgs {
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
        },
        vec![
            ("x-codex-tenant-id".to_string(), "tenant-a".to_string()),
            ("x-codex-project-id".to_string(), "project-b".to_string()),
        ],
    )
    .await
    .expect("other-project client should connect to multi-worker gateway");

    let second_started: AppServerThreadStartResponse = other_project_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(6),
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
        .expect("other-project thread/start should succeed through multi-worker remote gateway");
    assert_eq!(second_started.thread.id, "thread-worker-b");

    let other_project_list: AppServerThreadListResponse = other_project_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(7),
            params: ThreadListParams {
                parent_thread_id: None,
                ancestor_thread_id: None,
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
        .expect("other-project thread/list should succeed through multi-worker gateway");
    assert_eq!(
        other_project_list
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b"]
    );

    let other_project_loaded: ThreadLoadedListResponse = other_project_client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(8),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("other-project thread/loaded/list should succeed through multi-worker gateway");
    assert_eq!(other_project_loaded.data, vec!["thread-worker-b"]);

    let other_project_read_error = other_project_client
        .request_typed::<AppServerThreadReadResponse>(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(9),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect_err("other-project thread/read should be scope-filtered");
    assert_eq!(
        other_project_read_error.to_string(),
        format!(
            "thread/read failed: thread not found: {} (code -32602)",
            first_started.thread.id
        )
    );
    let TypedRequestError::Server {
        source: other_project_read_source,
        ..
    } = other_project_read_error
    else {
        panic!("other-project thread/read should return a server JSON-RPC error");
    };
    assert_eq!(
        other_project_read_source.message,
        format!("thread not found: {}", first_started.thread.id)
    );

    let other_tenant_client = RemoteAppServerClient::connect_with_headers(
        RemoteAppServerConnectArgs {
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
        },
        vec![
            ("x-codex-tenant-id".to_string(), "tenant-b".to_string()),
            ("x-codex-project-id".to_string(), "project-a".to_string()),
        ],
    )
    .await
    .expect("other-tenant client should connect to multi-worker gateway");

    let other_tenant_list: AppServerThreadListResponse = other_tenant_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(10),
            params: ThreadListParams {
                parent_thread_id: None,
                ancestor_thread_id: None,
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
        .expect("other-tenant thread/list should succeed through multi-worker gateway");
    assert_eq!(other_tenant_list.data.is_empty(), true);

    let healthz_response = reqwest::get(format!("http://{}/healthz", server.local_addr()))
        .await
        .expect("healthz response");
    assert_eq!(healthz_response.status(), reqwest::StatusCode::OK);
    let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
    assert_eq!(health.remote_account_labels_complete, Some(true));
    assert_eq!(health.remote_unlabeled_account_worker_count, Some(0));
    assert_eq!(health.remote_unlabeled_account_worker_ids, Some(Vec::new()));
    assert_eq!(health.remote_unlabeled_account_workers, Some(Vec::new()));
    let remote_workers = health.remote_workers.as_ref().expect("remote workers");
    assert_eq!(remote_workers.len(), 2);
    assert_eq!(remote_workers[0].account_id.as_deref(), Some("acct-a"));
    assert_eq!(remote_workers[1].account_id.as_deref(), Some("acct-b"));
    assert_eq!(
        health.project_worker_routes,
        Some(vec![
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 0,
                account_id: Some("acct-a".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
            crate::api::GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-b".to_string(),
                worker_id: 1,
                account_id: Some("acct-b".to_string()),
                account_capacity: crate::api::GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            },
        ])
    );

    assert_remote_client_shutdown(owner_client.shutdown().await);
    assert_remote_client_shutdown(same_scope_client.shutdown().await);
    assert_remote_client_shutdown(other_project_client.shutdown().await);
    assert_remote_client_shutdown(other_tenant_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
