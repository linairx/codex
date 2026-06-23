use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_turn_control_workflow() {
    #[cfg(target_os = "windows")]
    let shell_command = vec![
        "powershell".to_string(),
        "-Command".to_string(),
        "Start-Sleep -Seconds 10".to_string(),
    ];
    #[cfg(not(target_os = "windows"))]
    let shell_command = vec!["sleep".to_string(), "10".to_string()];

    let codex_home = tempdir().expect("tempdir");
    std::fs::write(codex_home.path().join("tracked.txt"), "review target\n")
        .expect("tracked file should be written");
    let init_output = std::process::Command::new("git")
        .arg("init")
        .current_dir(codex_home.path())
        .output()
        .expect("git init should succeed");
    assert_eq!(init_output.status.success(), true);
    let name_output = std::process::Command::new("git")
        .args(["config", "user.name", "Codex Test"])
        .current_dir(codex_home.path())
        .output()
        .expect("git user.name should be configured");
    assert_eq!(name_output.status.success(), true);
    let email_output = std::process::Command::new("git")
        .args(["config", "user.email", "codex@example.com"])
        .current_dir(codex_home.path())
        .output()
        .expect("git user.email should be configured");
    assert_eq!(email_output.status.success(), true);
    let add_output = std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(codex_home.path())
        .output()
        .expect("git add should succeed");
    assert_eq!(add_output.status.success(), true);
    let commit_output = std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(codex_home.path())
        .output()
        .expect("git commit should succeed");
    assert_eq!(commit_output.status.success(), true);
    let model_server = start_mock_responses_server_sequence(vec![
        create_shell_command_sse_response(
            shell_command,
            Some(codex_home.path()),
            Some(10_000),
            "call-sleep",
        )
        .expect("shell command response should serialize"),
    ])
    .await;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "workspace-write"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            model_server.uri
        ),
    )
    .expect("config.toml should be written");
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
    .expect("remote client should connect to embedded gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let turn_started_response: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "run sleep".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().to_path_buf()),
                ..Default::default()
            },
        })
        .await
        .expect("turn/start should succeed through embedded gateway");
    let turn_id = turn_started_response.turn.id.clone();

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
                && notification.turn.id == turn_id
            {
                break;
            }
        }
    })
    .await
    .expect("turn/started notification should arrive");

    let steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(3),
            params: TurnSteerParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer toward shutdown".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should succeed through embedded gateway");
    assert_eq!(steer_response.turn_id, turn_id);

    let interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(4),
            params: TurnInterruptParams {
                thread_id: started.thread.id.clone(),
                turn_id: turn_id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should succeed through embedded gateway");
    assert_eq!(interrupt_response, TurnInterruptResponse {});

    let mut saw_turn_completed = false;
    timeout(Duration::from_secs(10), async {
        while !saw_turn_completed {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == started.thread.id
                && turn.id == turn_id
                && turn.status == TurnStatus::Interrupted
            {
                saw_turn_completed = true;
            }
        }
    })
    .await
    .expect("turn control notifications should arrive");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_turn_control_workflow() {
    let websocket_url = start_mock_remote_multi_connection_turn_control_server(
        "thread-remote-workflow",
        "/tmp/remote-project",
        "remote steer delta",
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

    let turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start turn".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start should succeed through remote gateway");
    assert_eq!(turn_started.turn.id, "turn-thread-remote-workflow");

    let steer_response: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(3),
            params: TurnSteerParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "continue".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("turn/steer should succeed through remote gateway");
    assert_eq!(steer_response.turn_id, turn_started.turn.id);

    let interrupt_response: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(4),
            params: TurnInterruptParams {
                thread_id: started.thread.id.clone(),
                turn_id: turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("turn/interrupt should succeed through remote gateway");
    assert_eq!(interrupt_response, TurnInterruptResponse {});

    let mut coverage = TurnControlCoverage::default();
    timeout(Duration::from_secs(5), async {
        while coverage
            != (TurnControlCoverage {
                saw_thread_active: true,
                saw_turn_started: true,
                saw_agent_delta: true,
                saw_turn_completed: true,
                saw_thread_idle: true,
            })
        {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => match notification.status {
                    ThreadStatus::Active { ref active_flags }
                        if notification.thread_id == started.thread.id
                            && active_flags.is_empty() =>
                    {
                        coverage.saw_thread_active = true;
                    }
                    ThreadStatus::Idle if notification.thread_id == started.thread.id => {
                        coverage.saw_thread_idle = true;
                    }
                    _ => {}
                },
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_started.turn.id =>
                {
                    coverage.saw_turn_started = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn_id == turn_started.turn.id
                    && notification.delta == "remote steer delta" =>
                {
                    coverage.saw_agent_delta = true;
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) if notification.thread_id == started.thread.id
                    && notification.turn.id == turn_started.turn.id
                    && notification.turn.status == TurnStatus::Completed =>
                {
                    coverage.saw_turn_completed = true;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("turn control notifications should arrive");

    assert_eq!(
        coverage,
        TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        }
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_single_worker_supports_drop_in_v2_client_thread_control_and_review_workflow() {
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
        channel_capacity: 64,
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

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed through remote gateway");
    assert_eq!(
        unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(3),
            params: ThreadCompactStartParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed through remote gateway");
    assert_eq!(compact, ThreadCompactStartResponse {});

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(4),
            params: ThreadShellCommandParams {
                thread_id: started.thread.id.clone(),
                command: "git status --short".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed through remote gateway");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(5),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed through remote gateway");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(6),
            params: ThreadRollbackParams {
                thread_id: started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed through remote gateway");
    assert_eq!(rollback.thread.id, started.thread.id);
    assert_eq!(rollback.thread.preview, "/tmp/remote-project");
    assert_eq!(rollback.thread.turns, Vec::new());

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(7),
            params: ThreadArchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed through remote gateway");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(8),
            params: ThreadUnarchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed through remote gateway");
    assert_eq!(unarchive.thread.id, started.thread.id);
    assert_eq!(unarchive.thread.preview, "/tmp/remote-project");

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(9),
            params: ThreadMetadataUpdateParams {
                thread_id: started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed through remote gateway");
    assert_eq!(metadata_update.thread.id, started.thread.id);
    assert_eq!(metadata_update.thread.preview, "/tmp/remote-project");

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(10),
            params: ThreadTurnsListParams {
                thread_id: started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed through remote gateway");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, "turn-remote-workflow");
    assert_eq!(turns.next_cursor, None);

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(11),
            params: ThreadIncrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed through remote gateway");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(12),
            params: ThreadDecrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed through remote gateway");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(13),
            params: ThreadInjectItemsParams {
                thread_id: started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed through remote gateway");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(14),
            params: ReviewStartParams {
                thread_id: started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review the current change".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should succeed through remote gateway");
    assert_eq!(review.turn.id, "turn-review-remote-workflow");
    assert_eq!(review.review_thread_id, "thread-review-remote-workflow");

    let review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(15),
            params: ThreadReadParams {
                thread_id: review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for review thread through remote gateway");
    assert_eq!(review_thread.thread.id, review.review_thread_id);
    assert_eq!(review_thread.thread.preview, "/tmp/remote-project/review");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
