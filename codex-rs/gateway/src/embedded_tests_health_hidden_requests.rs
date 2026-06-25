use super::*;
use pretty_assertions::assert_eq;

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
