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
                    internal_chat_message_metadata_passthrough: None,
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
