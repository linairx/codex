use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_supports_v2_thread_reentry_from_later_client_session() {
    let websocket_url = start_mock_remote_workflow_server().await;
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

    let owner_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("owner client should connect to remote gateway");

    let started: AppServerThreadStartResponse = owner_client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                cwd: Some("/tmp/remote-project".to_string()),
                ephemeral: Some(true),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through remote gateway");
    assert_eq!(started.thread.id, "thread-remote-workflow");

    assert_remote_client_shutdown(owner_client.shutdown().await);

    let reentry_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("reentry client should connect to remote gateway");

    let listed: AppServerThreadListResponse = reentry_client
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
        .expect("thread/list should succeed for later remote client");
    assert_eq!(listed.data.len(), 1);
    assert_eq!(listed.data[0].id, started.thread.id);

    let read: AppServerThreadReadResponse = reentry_client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(3),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for later remote client");
    assert_eq!(read.thread.id, started.thread.id);

    assert_remote_client_shutdown(reentry_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
