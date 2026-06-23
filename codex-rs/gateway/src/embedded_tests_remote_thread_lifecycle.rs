use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_preserves_unmaterialized_thread_resume_and_fork_errors_over_v2() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to embedded gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some(codex_home.path().display().to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let resume_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(2),
            params: ThreadResumeParams {
                thread_id: started.thread.id.clone(),
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
        .expect_err("unmaterialized thread/resume should fail through embedded gateway");
    assert_eq!(
        resume_error.to_string(),
        format!(
            "thread/resume failed: no rollout found for thread id {} (code -32600)",
            started.thread.id
        )
    );
    let TypedRequestError::Server {
        source: resume_source,
        ..
    } = resume_error
    else {
        panic!("thread/resume should return a server JSON-RPC error");
    };
    assert_eq!(
        resume_source.message,
        format!("no rollout found for thread id {}", started.thread.id)
    );

    let resume_path_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(3),
            params: ThreadResumeParams {
                thread_id: started.thread.id.clone(),
                history: None,
                path: Some("/tmp/rollout.jsonl".into()),
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
        .expect_err("unknown path-based thread/resume should fail through embedded gateway");
    assert_eq!(
        resume_path_error.to_string(),
        "thread/resume failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    let fork_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(4),
            params: ThreadForkParams {
                thread_id: started.thread.id.clone(),
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
        .expect_err("unmaterialized thread/fork should fail through embedded gateway");
    assert_eq!(
        fork_error.to_string(),
        format!(
            "thread/fork failed: no rollout found for thread id {} (code -32600)",
            started.thread.id
        )
    );
    let TypedRequestError::Server {
        source: fork_source,
        ..
    } = fork_error
    else {
        panic!("thread/fork should return a server JSON-RPC error");
    };
    assert_eq!(
        fork_source.message,
        format!("no rollout found for thread id {}", started.thread.id)
    );

    let fork_path_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
            params: ThreadForkParams {
                thread_id: started.thread.id.clone(),
                path: Some("/tmp/rollout.jsonl".into()),
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
        .expect_err("unknown path-based thread/fork should fail through embedded gateway");
    assert_eq!(
        fork_path_error.to_string(),
        "thread/fork failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
