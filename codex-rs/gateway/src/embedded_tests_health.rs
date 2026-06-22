use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_resume_and_fork_routing() {
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

    let resumed: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(3),
            params: ThreadResumeParams {
                thread_id: first_started.thread.id.clone(),
                history: None,
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect("thread/resume should route to worker A");
    assert_eq!(resumed.thread.id, first_started.thread.id);
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a");

    let resumed_from_history: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: "thread-history-placeholder".to_string(),
                history: Some(vec![ResponseItem::Message {
                    id: None,
                    metadata: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "resume from explicit history".to_string(),
                    }],
                    phase: None,
                }]),
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect("history-based thread/resume should succeed through multi-worker gateway");
    assert_eq!(resumed_from_history.thread.id.is_empty(), false);
    assert_eq!(
        resumed_from_history.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );

    let first_rollout_path = first_started
        .thread
        .path
        .clone()
        .expect("materialized thread should include a rollout path");

    let resumed_from_path: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(5),
            params: ThreadResumeParams {
                thread_id: first_started.thread.id.clone(),
                history: None,
                path: Some(first_rollout_path.clone()),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect("path-based thread/resume should succeed through multi-worker gateway");
    assert_eq!(resumed_from_path.thread.id, first_started.thread.id);
    assert_eq!(resumed_from_path.thread.path, Some(first_rollout_path));

    let forked: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(6),
            params: ThreadForkParams {
                thread_id: second_started.thread.id.clone(),
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/fork should route to worker B");
    assert_eq!(forked.thread.id, "thread-worker-b-fork");
    assert_eq!(forked.cwd.as_ref().to_string_lossy(), "/tmp/worker-b-fork");

    let second_rollout_path = second_started
        .thread
        .path
        .clone()
        .expect("materialized thread should include a rollout path");

    let forked_from_path: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(7),
            params: ThreadForkParams {
                thread_id: second_started.thread.id.clone(),
                path: Some(second_rollout_path),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect("path-based thread/fork should succeed through multi-worker gateway");
    assert_eq!(
        forked_from_path.thread.forked_from_id,
        Some(second_started.thread.id)
    );
    assert_eq!(forked_from_path.thread.path.is_some(), true);

    let forked_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: forked_from_path.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to the forking worker");
    assert_eq!(forked_read.thread.id, forked_from_path.thread.id);
    assert_eq!(
        forked_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b-fork"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

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

#[tokio::test]
async fn remote_multi_worker_rejects_hidden_downstream_server_requests_across_scopes() {
    let (worker_a, hidden_request_rejection_rx) =
        start_mock_remote_multi_connection_hidden_server_request_server().await;
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

    let started: AppServerThreadStartResponse = owner_client
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
        .expect("owner thread/start should succeed through multi-worker remote gateway");
    assert_eq!(started.thread.id, "thread-worker-a");

    assert_remote_client_shutdown(owner_client.shutdown().await);

    let hidden_scope_client = RemoteAppServerClient::connect_with_headers(
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
            ("x-codex-project-id".to_string(), "project-b".to_string()),
        ],
    )
    .await
    .expect("hidden-scope client should connect to multi-worker gateway");
    let mut hidden_scope_client = hidden_scope_client;

    assert!(
        timeout(Duration::from_millis(200), hidden_scope_client.next_event())
            .await
            .is_err(),
        "hidden downstream server request should not be forwarded northbound"
    );

    let hidden_list: AppServerThreadListResponse = hidden_scope_client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(2),
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
        .expect("hidden-scope thread/list should remain usable after rejection");
    assert_eq!(hidden_list.next_cursor, None);
    assert!(hidden_list.data.is_empty());

    assert_eq!(
        timeout(Duration::from_secs(5), hidden_request_rejection_rx)
            .await
            .expect("hidden server request rejection should finish in time")
            .expect("hidden server request rejection should be observed"),
        "thread not found".to_string()
    );

    assert_remote_client_shutdown(hidden_scope_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_v2_session_survives_one_worker_disconnect() {
    let worker_a = start_mock_remote_multi_connection_disconnect_after_thread_start_server(
        "thread-worker-a",
        "/tmp/worker-a",
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
        .expect("second thread/start should still succeed after one worker disconnects");
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
        .expect("thread/list should still succeed after one worker disconnects");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b"]
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_after_disconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_thread_server_for_same_session_recovery(
        "thread-worker-a-3",
        "/tmp/worker-a-3",
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
    .expect("worker should reconnect before same-session recovery test starts");

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
        channel_capacity: 64,
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
    let recovered_started = match (
        first_started.thread.id.as_str(),
        second_started.thread.id.as_str(),
    ) {
        ("thread-worker-a-3", "thread-worker-b") => &first_started,
        ("thread-worker-b", "thread-worker-a-3") => &second_started,
        (first, second) => panic!("unexpected thread ids after reconnect: {first}, {second}"),
    };
    let recovered_thread_id = recovered_started.thread.id.clone();

    let first_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(3),
            params: ThreadReadParams {
                thread_id: recovered_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should route to the re-added worker");
    assert_eq!(first_read.thread.id, recovered_thread_id);
    assert_eq!(first_read.thread.preview, "/tmp/worker-a-3");

    let resumed: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
            params: ThreadResumeParams {
                thread_id: recovered_thread_id.clone(),
                history: None,
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect("thread/resume should route to the re-added worker");
    assert_eq!(resumed.thread.id, recovered_thread_id);
    assert_eq!(resumed.cwd.as_ref().to_string_lossy(), "/tmp/worker-a-3");

    let rollout_path = recovered_started
        .thread
        .path
        .clone()
        .expect("recovered thread should include a rollout path");
    let resumed_from_path: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(104),
            params: ThreadResumeParams {
                thread_id: recovered_thread_id.clone(),
                history: None,
                path: Some(rollout_path.clone()),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ..Default::default()
            },
        })
        .await
        .expect("path-based thread/resume should route to the re-added worker");
    assert_eq!(resumed_from_path.thread.id, recovered_thread_id);
    assert_eq!(resumed_from_path.thread.path, Some(rollout_path.clone()));

    let forked: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: recovered_thread_id.clone(),
                path: None,
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/fork should route to the re-added worker");
    assert_eq!(forked.thread.id, "thread-worker-a-3-fork");
    assert_eq!(
        forked.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a-3-fork"
    );
    let forked_from_path: ThreadForkResponse = client
        .request_typed(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(105),
            params: ThreadForkParams {
                thread_id: recovered_thread_id.clone(),
                path: Some(rollout_path),
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                base_instructions: None,
                developer_instructions: None,
                ephemeral: false,
                ..Default::default()
            },
        })
        .await
        .expect("path-based thread/fork should route to the re-added worker");
    assert_eq!(
        forked_from_path.thread.forked_from_id,
        Some(recovered_thread_id.clone())
    );
    assert_eq!(forked_from_path.thread.path.is_some(), true);

    let forked_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: forked.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("forked thread read should route to the re-added worker");
    assert_eq!(forked_read.thread.id, forked.thread.id);
    assert_eq!(forked_read.thread.preview, "/tmp/worker-a-3-fork");

    let first_apps: AppsListResponse = client
        .request_typed(ClientRequest::AppsList {
            request_id: RequestId::Integer(7),
            params: AppsListParams {
                cursor: None,
                limit: Some(10),
                thread_id: Some(recovered_thread_id.clone()),
                force_refetch: false,
            },
        })
        .await
        .expect("thread-scoped app/list should route to the re-added worker");
    assert_eq!(first_apps.next_cursor, None);
    assert_eq!(
        first_apps
            .data
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-3-app"]
    );

    let mcp_resource: McpResourceReadResponse = client
        .request_typed(ClientRequest::McpResourceRead {
            request_id: RequestId::Integer(107),
            params: McpResourceReadParams {
                thread_id: Some(recovered_thread_id.clone()),
                server: "thread-mcp".to_string(),
                uri: "file:///tmp/worker-a-3/context.md".to_string(),
            },
        })
        .await
        .expect("mcpServer/resource/read should route to the re-added worker");
    assert_eq!(
        serde_json::to_value(&mcp_resource.contents).expect("contents should serialize"),
        serde_json::json!([{
            "uri": "file:///tmp/worker-a-3/context.md",
            "mimeType": "text/markdown",
            "text": "thread-worker-a-3 recovered resource",
        }])
    );

    let mcp_tool: McpServerToolCallResponse = client
        .request_typed(ClientRequest::McpServerToolCall {
            request_id: RequestId::Integer(108),
            params: McpServerToolCallParams {
                thread_id: recovered_thread_id.clone(),
                server: "thread-mcp".to_string(),
                tool: "lookup".to_string(),
                arguments: Some(serde_json::json!({ "query": "worker-a-3" })),
                meta: None,
            },
        })
        .await
        .expect("mcpServer/tool/call should route to the re-added worker");
    assert_eq!(
        mcp_tool.structured_content,
        Some(serde_json::json!({ "threadId": "thread-worker-a-3" }))
    );
    assert_eq!(mcp_tool.is_error, Some(false));

    let renamed_thread_name = "Recovered Worker Thread".to_string();
    let rename_response: ThreadSetNameResponse = client
        .request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(8),
            params: ThreadSetNameParams {
                thread_id: recovered_thread_id.clone(),
                name: renamed_thread_name.clone(),
            },
        })
        .await
        .expect("thread/name/set should route to the re-added worker");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let renamed_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(9),
            params: ThreadReadParams {
                thread_id: recovered_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read after rename should still route to the re-added worker");
    assert_eq!(renamed_read.thread.id, "thread-worker-a-3");
    assert_eq!(renamed_read.thread.preview, "/tmp/worker-a-3");
    assert_eq!(renamed_read.thread.name, Some(renamed_thread_name));

    let rename_notification = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && notification.thread_id == recovered_thread_id
            {
                break notification;
            }
        }
    })
    .await
    .expect("thread/name/updated should fan in after same-session recovery");
    assert_eq!(rename_notification.thread_name, renamed_read.thread.name);

    let memory_mode_response: ThreadMemoryModeSetResponse = client
        .request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(10),
            params: ThreadMemoryModeSetParams {
                thread_id: first_started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("thread/memoryMode/set should route to the re-added worker");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    let unsubscribe_response: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(11),
            params: ThreadUnsubscribeParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should route to the re-added worker");
    assert_eq!(
        unsubscribe_response,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let archive_response: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(12),
            params: ThreadArchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should route to the re-added worker");
    assert_eq!(archive_response, ThreadArchiveResponse {});

    let unarchive_response: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(13),
            params: ThreadUnarchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should route to the re-added worker");
    assert_eq!(unarchive_response.thread.id, first_started.thread.id);
    assert_eq!(unarchive_response.thread.preview, "/tmp/worker-a-3");

    let expected_thread_lifecycle_notifications = HashSet::from([
        (first_started.thread.id.clone(), "closed"),
        (first_started.thread.id.clone(), "archived"),
        (first_started.thread.id.clone(), "unarchived"),
    ]);
    let mut thread_lifecycle_notifications = HashSet::new();
    let lifecycle_result = timeout(Duration::from_secs(5), async {
        while thread_lifecycle_notifications != expected_thread_lifecycle_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "closed"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadArchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "archived"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadUnarchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id => {
                    thread_lifecycle_notifications.insert((notification.thread_id, "unarchived"));
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        lifecycle_result.is_ok(),
        "thread lifecycle notifications should fan in after same-session recovery: {thread_lifecycle_notifications:?}"
    );

    let metadata_update_response: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(14),
            params: ThreadMetadataUpdateParams {
                thread_id: first_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-recovered-worker".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("thread/metadata/update should route to the re-added worker");
    assert_eq!(metadata_update_response.thread.id, first_started.thread.id);
    assert_eq!(metadata_update_response.thread.preview, "/tmp/worker-a-3");

    let turns_response: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(15),
            params: ThreadTurnsListParams {
                thread_id: first_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should route to the re-added worker");
    assert_eq!(turns_response.data.len(), 1);
    assert_eq!(turns_response.data[0].id, "turn-thread-worker-a-3");
    assert_eq!(turns_response.next_cursor, None);
    assert_eq!(turns_response.backwards_cursor, None);

    let increment_response: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadIncrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should route to the re-added worker");
    assert_eq!(
        increment_response,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_response: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(17),
            params: ThreadDecrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should route to the re-added worker");
    assert_eq!(
        decrement_response,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_response: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(18),
            params: ThreadInjectItemsParams {
                thread_id: first_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Recovered worker inject"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should route to the re-added worker");
    assert_eq!(inject_response, ThreadInjectItemsResponse {});

    let compact_response: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should route to the re-added worker");
    assert_eq!(compact_response, ThreadCompactStartResponse {});

    let shell_command_response: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(20),
            params: ThreadShellCommandParams {
                thread_id: first_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should route to the re-added worker");
    assert_eq!(shell_command_response, ThreadShellCommandResponse {});

    let background_clean_response: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(21),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should route to the re-added worker");
    assert_eq!(
        background_clean_response,
        ThreadBackgroundTerminalsCleanResponse {}
    );

    let rollback_response: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(22),
            params: ThreadRollbackParams {
                thread_id: first_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should route to the re-added worker");
    assert_eq!(rollback_response.thread.id, first_started.thread.id);
    assert_eq!(rollback_response.thread.preview, "/tmp/worker-a-3");

    let review_response: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(23),
            params: ReviewStartParams {
                thread_id: first_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review recovered worker".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should route to the re-added worker");
    assert_eq!(review_response.turn.id, "turn-review-thread-worker-a-3");
    assert_eq!(review_response.review_thread_id, "thread-worker-a-3-review");

    let review_thread_read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(24),
            params: ThreadReadParams {
                thread_id: review_response.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("review thread read should route to the re-added worker");
    assert_eq!(
        review_thread_read.thread.id,
        review_response.review_thread_id
    );
    assert_eq!(review_thread_read.thread.preview, "/tmp/worker-a-3");

    let turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(25),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Recovered worker turn".to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should route to the re-added worker");
    assert_eq!(turn_started.turn.id, "turn-thread-worker-a-3");

    let mut recovered_turn_coverage = TurnStreamingCoverage::default();
    let mut recovered_extended_turn_notifications = HashSet::new();
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
            || recovered_extended_turn_notifications != expected_extended_turn_notifications()
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
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
                    && notification.turn.id == turn_started.turn.id =>
                {
                    recovered_turn_coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::HookStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some(turn_started.turn.id.as_str())
                    && notification.run.id == format!("hook-{}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_hook_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", first_started.thread.id)
                            && text == "streaming answer in progress"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == "hello from recovered worker" =>
                {
                    recovered_turn_coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::PlanDelta(notification))
                    if notification.thread_id == first_started.thread.id
                        && notification.turn_id == turn_started.turn.id
                        && notification.delta == format!("plan {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("plan_delta");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryTextDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("summary {}", first_started.thread.id)
                    && notification.summary_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_summary_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ReasoningSummaryPartAdded(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.summary_index == 0 =>
                {
                    recovered_extended_turn_notifications.insert("reasoning_summary_part_added");
                }
                AppServerEvent::ServerNotification(ServerNotification::ReasoningTextDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("reasoning {}", first_started.thread.id)
                    && notification.content_index == 0 =>
                {
                    recovered_turn_coverage.saw_reasoning_text_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TerminalInteraction(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.process_id == format!("proc-{}", first_started.thread.id)
                    && notification.stdin == "y\n" =>
                {
                    recovered_extended_turn_notifications.insert("terminal_interaction");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::CommandExecutionOutputDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("stdout {}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_command_output_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::FileChangeOutputDelta(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == format!("patch {}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_file_change_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnDiffUpdated(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.diff == format!("diff {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("turn_diff_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnPlanUpdated(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.explanation.as_deref() == Some("gateway multi-worker plan")
                    && notification.plan.len() == 1
                    && notification.plan[0].step == format!("plan {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("turn_plan_updated");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadTokenUsageUpdated(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.token_usage.total.total_tokens == 42 =>
                {
                    recovered_extended_turn_notifications.insert("thread_token_usage_updated");
                }
                AppServerEvent::ServerNotification(ServerNotification::McpToolCallProgress(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.message
                        == format!("mcp progress {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("mcp_tool_call_progress");
                }
                AppServerEvent::ServerNotification(ServerNotification::ContextCompacted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id =>
                {
                    recovered_extended_turn_notifications.insert("context_compacted");
                }
                AppServerEvent::ServerNotification(ServerNotification::ModelRerouted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.from_model == "gpt-5"
                    && notification.to_model == "gpt-5-codex" =>
                {
                    recovered_extended_turn_notifications.insert("model_rerouted");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::RawResponseItemCompleted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id =>
                {
                    recovered_extended_turn_notifications.insert("raw_response_item_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::Error(notification))
                    if notification.thread_id == first_started.thread.id
                        && notification.turn_id == turn_started.turn.id
                        && !notification.will_retry
                        && notification.error.message
                            == format!("recoverable warning {}", first_started.thread.id) =>
                {
                    recovered_extended_turn_notifications.insert("error");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewStarted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.review_id
                        == format!("guardian-{}", first_started.thread.id)
                    && notification.target_item_id.as_deref()
                        == Some(&format!("cmd-{}", first_started.thread.id)) =>
                {
                    recovered_extended_turn_notifications.insert("guardian_review_started");
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ItemGuardianApprovalReviewCompleted(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.review_id
                        == format!("guardian-{}", first_started.thread.id)
                    && notification.target_item_id.as_deref()
                        == Some(&format!("cmd-{}", first_started.thread.id)) =>
                {
                    recovered_extended_turn_notifications.insert("guardian_review_completed");
                }
                AppServerEvent::ServerNotification(ServerNotification::HookCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id.as_deref() == Some(turn_started.turn.id.as_str())
                    && notification.run.id == format!("hook-{}", first_started.thread.id) =>
                {
                    recovered_turn_coverage.saw_hook_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && matches!(
                        &notification.item,
                        ThreadItem::AgentMessage {
                            id,
                            text,
                            ..
                        } if id == &format!("msg-{}", first_started.thread.id)
                            && text == "streaming answer completed"
                    ) =>
                {
                    recovered_turn_coverage.saw_item_completed = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.turn.id == turn_started.turn.id =>
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
        "turn notifications should fan in after same-session recovery: lifecycle={recovered_turn_coverage:?} extended={recovered_extended_turn_notifications:?}"
    );

    let turn_steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(26),
            params: TurnSteerParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "Steer recovered worker".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should route to the re-added worker");
    assert_eq!(turn_steer_response.turn_id, turn_started.turn.id);

    let turn_interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(27),
            params: TurnInterruptParams {
                thread_id: first_started.thread.id.clone(),
                turn_id: turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should route to the re-added worker");
    assert_eq!(turn_interrupt_response, TurnInterruptResponse {});

    let realtime_start_response: ThreadRealtimeStartResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStart {
            request_id: RequestId::Integer(28),
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
        .expect("thread/realtime/start should route to the re-added worker");
    assert_eq!(realtime_start_response, ThreadRealtimeStartResponse {});

    let realtime_append_text_response: ThreadRealtimeAppendTextResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendText {
            request_id: RequestId::Integer(29),
            params: ThreadRealtimeAppendTextParams {
                thread_id: first_started.thread.id.clone(),
                text: "Recovered worker realtime text".to_string(),
                ..Default::default()
            },
        })
        .await
        .expect("thread/realtime/appendText should route to the re-added worker");
    assert_eq!(
        realtime_append_text_response,
        ThreadRealtimeAppendTextResponse {}
    );

    let realtime_append_audio_response: ThreadRealtimeAppendAudioResponse = client
        .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
            request_id: RequestId::Integer(30),
            params: ThreadRealtimeAppendAudioParams {
                thread_id: first_started.thread.id.clone(),
                audio: ThreadRealtimeAudioChunk {
                    data: "AQID".to_string(),
                    sample_rate: 24_000,
                    num_channels: 1,
                    samples_per_channel: Some(3),
                    item_id: Some("recovered-worker-audio".to_string()),
                },
            },
        })
        .await
        .expect("thread/realtime/appendAudio should route to the re-added worker");
    assert_eq!(
        realtime_append_audio_response,
        ThreadRealtimeAppendAudioResponse {}
    );

    let realtime_stop_response: ThreadRealtimeStopResponse = client
        .request_typed(ClientRequest::ThreadRealtimeStop {
            request_id: RequestId::Integer(31),
            params: ThreadRealtimeStopParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/realtime/stop should route to the re-added worker");
    assert_eq!(realtime_stop_response, ThreadRealtimeStopResponse {});

    let mut recovered_realtime_coverage = RealtimeStreamingCoverage::default();
    timeout(Duration::from_secs(5), async {
        while recovered_realtime_coverage
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
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after same-session recovery");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeStarted(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.realtime_session_id.as_deref()
                        == Some(format!("session-{}", first_started.thread.id).as_str())
                    && notification.version == RealtimeConversationVersion::V2 =>
                {
                    recovered_realtime_coverage.saw_started = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeItemAdded(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.item["type"] == "message" =>
                {
                    recovered_realtime_coverage.saw_item_added = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeOutputAudioDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.audio.sample_rate == 24_000
                    && notification.audio.num_channels == 1
                    && notification.audio.data == "AQID" =>
                {
                    recovered_realtime_coverage.saw_output_audio_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDelta(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.delta == format!("delta {}", first_started.thread.id)
                    && notification.role == "assistant" =>
                {
                    recovered_realtime_coverage.saw_transcript_delta = true;
                }
                AppServerEvent::ServerNotification(
                    ServerNotification::ThreadRealtimeTranscriptDone(notification),
                ) if notification.thread_id == first_started.thread.id
                    && notification.text == format!("done {}", first_started.thread.id)
                    && notification.role == "assistant" =>
                {
                    recovered_realtime_coverage.saw_transcript_done = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeSdp(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.sdp.contains("s=Codex") =>
                {
                    recovered_realtime_coverage.saw_sdp = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeError(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.message == "realtime transport warning" =>
                {
                    recovered_realtime_coverage.saw_error = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadRealtimeClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    && notification.reason.as_deref() == Some("client requested stop") =>
                {
                    recovered_realtime_coverage.saw_closed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("realtime notifications should fan in after same-session recovery");

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(32),
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
        .expect("thread/list should include recovered and surviving workers");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a-3"]
    );

    let loaded_listed: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(33),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should include recovered and surviving workers");
    assert_eq!(loaded_listed.next_cursor, None);
    assert_eq!(
        loaded_listed
            .data
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["thread-worker-a-3", "thread-worker-b"]
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

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

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_worker_for_setup_mutations() {
    let (worker_a, worker_a_requests) =
        start_reconnecting_v2_multi_connection_session_mutation_server("worker-a").await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_multi_connection_session_mutation_server("worker-b").await;
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
    .expect("worker should reconnect before same-session setup mutation test starts");

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

    sleep(Duration::from_millis(200)).await;

    let imported: ExternalAgentConfigImportResponse = client
        .request_typed(ClientRequest::ExternalAgentConfigImport {
            request_id: RequestId::Integer(1),
            params: ExternalAgentConfigImportParams {
                migration_items: Vec::new(),
                source: None,
            },
        })
        .await
        .expect("externalAgentConfig/import should reconnect and fan out");
    assert!(!imported.import_id.is_empty());

    let import_completed = timeout(Duration::from_secs(5), client.next_event())
        .await
        .expect("externalAgentConfig/import/completed should finish in time")
        .expect("event stream should stay open after externalAgentConfig/import");
    assert!(matches!(
        import_completed,
        AppServerEvent::ServerNotification(ServerNotification::ExternalAgentConfigImportCompleted(
            _
        ))
    ));
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate externalAgentConfig/import/completed should be suppressed after recovered import fanout"
    );

    let batch_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigBatchWrite {
            request_id: RequestId::Integer(2),
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: Some("/tmp/shared/config.toml".to_string()),
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite should reconnect and fan out");
    assert_eq!(batch_write.version, "worker-a");

    let config_value_write: ConfigWriteResponse = client
        .request_typed(ClientRequest::ConfigValueWrite {
            request_id: RequestId::Integer(3),
            params: ConfigValueWriteParams {
                key_path: "plugins.shared-plugin".to_string(),
                value: serde_json::json!({
                    "enabled": true,
                }),
                merge_strategy: MergeStrategy::Upsert,
                file_path: None,
                expected_version: None,
            },
        })
        .await
        .expect("config/value/write should reconnect and fan out");
    assert_eq!(config_value_write.version, "worker-a");

    let marketplace: MarketplaceAddResponse = client
        .request_typed(ClientRequest::MarketplaceAdd {
            request_id: RequestId::Integer(4),
            params: MarketplaceAddParams {
                source: "https://example.com/shared-marketplace.git".to_string(),
                ref_name: Some("main".to_string()),
                sparse_paths: Some(vec!["plugins/shared-plugin".to_string()]),
            },
        })
        .await
        .expect("marketplace/add should reconnect and fan out");
    assert_eq!(marketplace.marketplace_name, "shared-marketplace");

    let skills_config: SkillsConfigWriteResponse = client
        .request_typed(ClientRequest::SkillsConfigWrite {
            request_id: RequestId::Integer(5),
            params: SkillsConfigWriteParams {
                path: Some(
                    PathBuf::from("/tmp/shared/skills/shared-skill")
                        .try_into()
                        .expect("skills/config/write path should be absolute"),
                ),
                name: None,
                enabled: true,
            },
        })
        .await
        .expect("skills/config/write should reconnect and fan out");
    assert_eq!(skills_config.effective_enabled, true);

    timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after skills/config/write");
            if matches!(
                event,
                AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
            ) {
                break;
            }
        }
    })
    .await
    .expect("skills/changed should arrive after recovered skills/config/write");
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate skills/changed should be suppressed after recovered skills/config/write"
    );

    let feature_enablement: ExperimentalFeatureEnablementSetResponse = client
        .request_typed(ClientRequest::ExperimentalFeatureEnablementSet {
            request_id: RequestId::Integer(6),
            params: ExperimentalFeatureEnablementSetParams {
                enablement: std::collections::BTreeMap::from([(
                    "gateway-test-feature".to_string(),
                    true,
                )]),
            },
        })
        .await
        .expect("experimentalFeature/enablement/set should reconnect and fan out");
    assert_eq!(
        feature_enablement.enablement,
        std::collections::BTreeMap::from([("gateway-test-feature".to_string(), true)])
    );

    let mcp_refresh: McpServerRefreshResponse = client
        .request_typed(ClientRequest::McpServerRefresh {
            request_id: RequestId::Integer(7),
            params: None,
        })
        .await
        .expect("config/mcpServer/reload should reconnect and fan out");
    assert_eq!(mcp_refresh, McpServerRefreshResponse {});

    let reset: MemoryResetResponse = client
        .request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(8),
            params: None,
        })
        .await
        .expect("memory/reset should reconnect and fan out");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = client
        .request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(9),
            params: None,
        })
        .await
        .expect("account/logout should reconnect and fan out");
    assert_eq!(logout, LogoutAccountResponse {});

    let logout_account_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after account/logout");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountUpdated(
                notification,
            )) = event
                && notification.auth_mode.is_none()
                && notification.plan_type.is_none()
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/updated should arrive after recovered account/logout fanout");
    assert_eq!(logout_account_updated.auth_mode, None);
    assert_eq!(logout_account_updated.plan_type, None);
    assert!(
        timeout(Duration::from_millis(200), client.next_event())
            .await
            .is_err(),
        "duplicate account/updated should be suppressed after recovered account/logout fanout"
    );

    let watch: FsWatchResponse = client
        .request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(10),
            params: FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared-repo/.git/HEAD")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        })
        .await
        .expect("fs/watch should reconnect and fan out");
    assert_eq!(
        watch.path.as_ref().to_string_lossy(),
        "/tmp/shared-repo/.git/HEAD"
    );

    let fs_changed_paths = timeout(Duration::from_secs(5), async {
        let mut changed_paths = HashSet::new();
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open after fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                if notification.watch_id != "watch-shared" {
                    continue;
                }
                changed_paths.extend(
                    notification
                        .changed_paths
                        .into_iter()
                        .map(|path| path.as_ref().to_string_lossy().into_owned()),
                );
                if changed_paths.len() == 2 {
                    break changed_paths;
                }
            }
        }
    })
    .await
    .expect("fs/changed notifications should arrive after recovered fs/watch");
    assert_eq!(
        fs_changed_paths,
        HashSet::from([
            "/tmp/worker-a/shared-repo/.git/HEAD".to_string(),
            "/tmp/worker-b/shared-repo/.git/HEAD".to_string(),
        ])
    );

    let unwatch: FsUnwatchResponse = client
        .request_typed(ClientRequest::FsUnwatch {
            request_id: RequestId::Integer(11),
            params: FsUnwatchParams {
                watch_id: "watch-shared".to_string(),
            },
        })
        .await
        .expect("fs/unwatch should reconnect and fan out");
    assert_eq!(unwatch, FsUnwatchResponse {});

    let expected_methods = vec![
        "externalAgentConfig/import".to_string(),
        "config/batchWrite".to_string(),
        "config/value/write".to_string(),
        "marketplace/add".to_string(),
        "skills/config/write".to_string(),
        "experimentalFeature/enablement/set".to_string(),
        "config/mcpServer/reload".to_string(),
        "memory/reset".to_string(),
        "account/logout".to_string(),
        "fs/watch".to_string(),
        "fs/unwatch".to_string(),
    ];
    assert_eq!(*worker_a_requests.lock().await, expected_methods);
    assert_eq!(*worker_b_requests.lock().await, expected_methods);

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[path = "embedded_tests_health_late.rs"]
mod embedded_tests_health_late;
