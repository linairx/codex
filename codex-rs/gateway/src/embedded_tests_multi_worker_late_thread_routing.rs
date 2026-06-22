use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_preserves_unmaterialized_thread_resume_and_fork_errors_over_v2() {
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
        .expect_err("unmaterialized thread/resume should fail through remote gateway");
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

    let resumed_from_history: ThreadResumeResponse = client
        .request_typed(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(3),
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
        .expect("history-based thread/resume should succeed through remote gateway");
    assert_eq!(resumed_from_history.thread.id.is_empty(), false);

    let resume_path_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadResume {
            request_id: RequestId::Integer(4),
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
        .expect_err("unknown path-based thread/resume should fail through remote gateway");
    assert_eq!(
        resume_path_error.to_string(),
        "thread/resume failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    let fork_error = client
        .request_typed::<serde_json::Value>(ClientRequest::ThreadFork {
            request_id: RequestId::Integer(5),
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
        .expect_err("unmaterialized thread/fork should fail through remote gateway");
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
            request_id: RequestId::Integer(6),
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
        .expect_err("unknown path-based thread/fork should fail through remote gateway");
    assert_eq!(
        fork_path_error.to_string(),
        "thread/fork failed: thread not found: /tmp/rollout.jsonl (code -32602)"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_runtime_round_robins_threads_across_workers() {
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

    let client = reqwest::Client::new();
    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-a".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first_thread: ThreadResponse = first_response.json().await.expect("first thread");

    let second_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some("/tmp/project-b".to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("second response");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_thread: ThreadResponse = second_response.json().await.expect("second thread");

    assert_eq!(first_thread.thread.id, "thread-worker-a");
    assert_eq!(second_thread.thread.id, "thread-worker-b");

    let list_response = client
        .get(format!(
            "http://{}/v1/threads?limit=10",
            server.local_addr()
        ))
        .send()
        .await
        .expect("list response");
    assert_eq!(list_response.status(), reqwest::StatusCode::OK);
    let list: ListThreadsResponse = list_response.json().await.expect("thread list");
    assert_eq!(list.data.len(), 2);
    assert_eq!(
        list.data
            .into_iter()
            .map(|thread| thread.id)
            .collect::<Vec<_>>(),
        vec!["thread-worker-b".to_string(), "thread-worker-a".to_string()]
    );

    server.shutdown().await.expect("shutdown");
}
