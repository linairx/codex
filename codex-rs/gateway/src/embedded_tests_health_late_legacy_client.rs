use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_legacy_client_requests_over_v2() {
    let worker_a = start_mock_remote_multi_connection_legacy_server("worker-a").await;
    let worker_b = start_mock_remote_multi_connection_legacy_server("worker-b").await;
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

    let _worker_a_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-a".to_string()),
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
        }),
    )
    .await
    .expect("first thread/start should finish in time")
    .expect("first thread/start should register worker A scope");

    let worker_b_thread: AppServerThreadStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/worker-b".to_string()),
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
        }),
    )
    .await
    .expect("second thread/start should finish in time")
    .expect("second thread/start should register worker B scope");
    let worker_b_rollout_path = worker_b_thread
        .thread
        .path
        .expect("worker B thread should expose rollout path");

    let summary: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(3),
            params: GetConversationSummaryParams::RolloutPath {
                rollout_path: worker_b_rollout_path.clone(),
            },
        }),
    )
    .await
    .expect("getConversationSummary should finish in time")
    .expect("getConversationSummary should route through multi-worker gateway");
    assert_eq!(
        summary.summary.conversation_id.to_string(),
        "00000000-0000-0000-0000-0000000000b2"
    );
    assert_eq!(summary.summary.path, worker_b_rollout_path);
    assert_eq!(summary.summary.preview, "Worker B summary");

    let summary_by_thread_id: GetConversationSummaryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetConversationSummary {
            request_id: RequestId::Integer(4),
            params: GetConversationSummaryParams::ThreadId {
                conversation_id: ThreadId::from_string(&worker_b_thread.thread.id)
                    .expect("worker B thread id should parse"),
            },
        }),
    )
    .await
    .expect("getConversationSummary by thread id should finish in time")
    .expect("getConversationSummary by thread id should route to the owning worker");
    assert_eq!(
        summary_by_thread_id.summary.conversation_id,
        ThreadId::from_string(&worker_b_thread.thread.id).expect("worker B thread id should parse")
    );
    assert_eq!(summary_by_thread_id.summary.path, worker_b_rollout_path);
    assert_eq!(summary_by_thread_id.summary.preview, "Worker B summary");

    let diff: GitDiffToRemoteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GitDiffToRemote {
            request_id: RequestId::Integer(5),
            params: GitDiffToRemoteParams {
                cwd: PathBuf::from("/tmp/worker-b/repo"),
            },
        }),
    )
    .await
    .expect("gitDiffToRemote should finish in time")
    .expect("gitDiffToRemote should route through multi-worker gateway");
    assert_eq!(diff.sha.0, "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(diff.diff, "diff --git a/README.md b/README.md\n");

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
