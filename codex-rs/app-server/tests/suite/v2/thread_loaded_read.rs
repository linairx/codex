use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::create_mock_responses_server_sequence;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_request_user_input_sse_response;
use app_test_support::create_shell_command_sse_response;
use app_test_support::rollout_path;
use app_test_support::to_response;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_app_server_protocol::interactive_thread_source_kinds;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::Settings;
use codex_protocol::openai_models::ReasoningEffort;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use std::path::Path;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_loaded_read_returns_loaded_thread_summaries() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, next_cursor } =
        to_response::<ThreadLoadedReadResponse>(resp)?;

    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, thread_id);
    assert_eq!(data[0].status, ThreadStatus::Idle);
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_paginates() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let first = start_thread(&mut mcp).await?;
    let second = start_thread(&mut mcp).await?;
    let mut expected = [first, second];
    expected.sort();

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams {
            cursor: None,
            limit: Some(1),
            model_providers: None,
            source_kinds: None,
            cwd: None,
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse {
        data: first_page,
        next_cursor,
    } = to_response::<ThreadLoadedReadResponse>(resp)?;
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].id, expected[0]);
    assert_eq!(next_cursor, Some(expected[0].clone()));

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams {
            cursor: next_cursor,
            limit: Some(1),
            model_providers: None,
            source_kinds: None,
            cwd: None,
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse {
        data: second_page,
        next_cursor,
    } = to_response::<ThreadLoadedReadResponse>(resp)?;
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].id, expected[1]);
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_filters_by_cwd() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let target_cwd = if cfg!(windows) {
        String::from(r"C:\srv\repo-a")
    } else {
        String::from("/srv/repo-a")
    };
    let other_cwd = if cfg!(windows) {
        String::from(r"C:\srv\repo-b")
    } else {
        String::from("/srv/repo-b")
    };

    let target_thread = start_thread_in_cwd(&mut mcp, &target_cwd).await?;
    let _other_thread = start_thread_in_cwd(&mut mcp, &other_cwd).await?;

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams {
            cursor: None,
            limit: None,
            model_providers: None,
            source_kinds: None,
            cwd: Some(target_cwd),
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, next_cursor } =
        to_response::<ThreadLoadedReadResponse>(resp)?;

    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, target_thread);
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_filters_by_model_provider_and_source_kind() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams {
            cursor: None,
            limit: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: Some(interactive_thread_source_kinds()),
            cwd: None,
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, next_cursor } =
        to_response::<ThreadLoadedReadResponse>(resp)?;

    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, thread_id);
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_reports_workspace_changed_for_resident_threads() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let req_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.1".to_string()),
            cwd: Some(workspace.path().display().to_string()),
            mode: Some(ThreadMode::ResidentAssistant),
            ..Default::default()
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(req_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(resp)?;
    assert!(thread.resident);
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    std::fs::write(workspace.path().join("watched.txt"), "changed")?;

    let loaded_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let read_id = mcp
                .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
                .await?;
            let resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(read_id))
                .await?;
            let ThreadLoadedReadResponse { data, .. } =
                to_response::<ThreadLoadedReadResponse>(resp)?;
            let loaded_thread = data
                .iter()
                .find(|loaded_thread| loaded_thread.id == thread.id)
                .expect("thread/loaded/read should include the resident thread");
            if let ThreadStatus::Active { active_flags } = &loaded_thread.status
                && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
            {
                return Ok::<ThreadStatus, anyhow::Error>(loaded_thread.status.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await??;

    assert_eq!(
        loaded_status,
        ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
        }
    );

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_reports_waiting_on_approval_for_resident_threads() -> Result<()> {
    let server = create_mock_responses_server_sequence_unchecked(vec![
        create_shell_command_sse_response(
            vec![
                "python3".to_string(),
                "-c".to_string(),
                "print(42)".to_string(),
            ],
            /*workdir*/ None,
            Some(5000),
            "call-approval",
        )?,
        app_test_support::create_final_assistant_message_sse_response("done")?,
    ])
    .await;
    let codex_home = TempDir::new()?;
    create_config_toml_with_approval_policy(codex_home.path(), &server.uri(), "untrusted")?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            mode: Some(ThreadMode::ResidentAssistant),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;
    assert!(thread.resident);
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "run command needing approval".to_string(),
                text_elements: Vec::new(),
            }],
            approval_policy: Some(AskForApproval::UnlessTrusted),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_start_resp)?;

    let original_request = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::CommandExecutionRequestApproval { request_id, .. } = original_request else {
        panic!("expected CommandExecutionRequestApproval request");
    };

    let loaded_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let loaded_read_id = mcp
                .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
                .await?;
            let loaded_read_resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(loaded_read_id))
                .await?;
            let ThreadLoadedReadResponse { data, .. } =
                to_response::<ThreadLoadedReadResponse>(loaded_read_resp)?;
            let loaded_thread = data
                .iter()
                .find(|loaded_thread| loaded_thread.id == thread.id)
                .expect("thread/loaded/read should include the resident thread");
            if let ThreadStatus::Active { active_flags } = &loaded_thread.status
                && active_flags.contains(&ThreadActiveFlag::WaitingOnApproval)
            {
                return Ok::<ThreadStatus, anyhow::Error>(loaded_thread.status.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await??;

    assert_eq!(
        loaded_status,
        ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WaitingOnApproval],
        }
    );

    mcp.send_response(
        request_id,
        serde_json::to_value(CommandExecutionRequestApprovalResponse {
            decision: CommandExecutionApprovalDecision::Accept,
        })?,
    )
    .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_loaded_read_reports_waiting_on_user_input_for_resident_threads() -> Result<()> {
    let codex_home = TempDir::new()?;
    let responses = vec![
        create_request_user_input_sse_response("call-user-input")?,
        app_test_support::create_final_assistant_message_sse_response("done")?,
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml_with_collaboration_modes(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            mode: Some(ThreadMode::ResidentAssistant),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;
    assert!(thread.resident);
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "ask something".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            effort: Some(ReasoningEffort::Medium),
            collaboration_mode: Some(CollaborationMode {
                mode: ModeKind::Plan,
                settings: Settings {
                    model: "mock-model".to_string(),
                    reasoning_effort: Some(ReasoningEffort::Medium),
                    developer_instructions: None,
                },
            }),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_start_resp)?;

    let original_request = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::ToolRequestUserInput { request_id, params } = original_request else {
        panic!("expected ToolRequestUserInput request");
    };
    assert_eq!(params.thread_id, thread.id);

    let loaded_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let loaded_read_id = mcp
                .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
                .await?;
            let loaded_read_resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(loaded_read_id))
                .await?;
            let ThreadLoadedReadResponse { data, .. } =
                to_response::<ThreadLoadedReadResponse>(loaded_read_resp)?;
            let loaded_thread = data
                .iter()
                .find(|loaded_thread| loaded_thread.id == thread.id)
                .expect("thread/loaded/read should include the resident thread");
            if let ThreadStatus::Active { active_flags } = &loaded_thread.status
                && active_flags.contains(&ThreadActiveFlag::WaitingOnUserInput)
            {
                return Ok::<ThreadStatus, anyhow::Error>(loaded_thread.status.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await??;

    assert_eq!(
        loaded_status,
        ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WaitingOnUserInput],
        }
    );

    mcp.send_response(
        request_id,
        serde_json::json!({
            "answers": {
                "confirm_path": { "answers": ["yes"] }
            }
        }),
    )
    .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_loaded_read_reports_background_terminal_for_resident_threads() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let responses = vec![
        create_background_exec_command_sse_response("uexec-bg")?,
        app_test_support::create_final_assistant_message_sse_response("done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;
    create_config_toml_with_unified_exec(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            mode: Some(ThreadMode::ResidentAssistant),
            cwd: Some(workspace.path().display().to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;
    assert!(thread.resident);
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "start a background process".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            sandbox_policy: Some(codex_app_server_protocol::SandboxPolicy::DangerFullAccess),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_start_resp)?;

    let loaded_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let loaded_read_id = mcp
                .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
                .await?;
            let loaded_read_resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(loaded_read_id))
                .await?;
            let ThreadLoadedReadResponse { data, .. } =
                to_response::<ThreadLoadedReadResponse>(loaded_read_resp)?;
            let loaded_thread = data
                .iter()
                .find(|loaded_thread| loaded_thread.id == thread.id)
                .expect("thread/loaded/read should include the resident thread");
            if let ThreadStatus::Active { active_flags } = &loaded_thread.status
                && active_flags.contains(&ThreadActiveFlag::BackgroundTerminalRunning)
            {
                return Ok::<ThreadStatus, anyhow::Error>(loaded_thread.status.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await??;

    assert_eq!(
        loaded_status,
        ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::BackgroundTerminalRunning],
        }
    );

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_reports_system_error_for_resident_threads() -> Result<()> {
    let responses = vec![
        app_test_support::create_final_assistant_message_sse_response("seeded")?,
        responses::sse_failed("resp-2", "server_error", "simulated failure"),
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            mode: Some(ThreadMode::ResidentAssistant),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;
    assert!(thread.resident);
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    let seed_turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "seed history".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let seed_turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(seed_turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(seed_turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let failed_turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "fail turn".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let failed_turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(failed_turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(failed_turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("error"),
    )
    .await??;

    let loaded_read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
        .await?;
    let loaded_read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(loaded_read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, .. } =
        to_response::<ThreadLoadedReadResponse>(loaded_read_resp)?;
    let loaded_thread = data
        .iter()
        .find(|loaded_thread| loaded_thread.id == thread.id)
        .expect("thread/loaded/read should include the resident thread");
    assert_eq!(loaded_thread.status, ThreadStatus::SystemError);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_preserves_preview_for_loaded_thread_resumed_from_external_rollout()
-> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let external_home = TempDir::new()?;
    let preview = "external loaded preview";
    let conversation_id = create_fake_rollout(
        external_home.path(),
        "2025-01-02T10-00-00",
        "2025-01-02T10:00:00Z",
        preview,
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let external_rollout_path = rollout_path(
        external_home.path(),
        "2025-01-02T10-00-00",
        &conversation_id,
    );

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id.clone(),
            path: Some(external_rollout_path.clone()),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(thread.preview, preview);

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, next_cursor } =
        to_response::<ThreadLoadedReadResponse>(read_resp)?;

    assert_eq!(next_cursor, None);
    let loaded = data
        .iter()
        .find(|candidate| candidate.id == conversation_id)
        .expect("thread/loaded/read should include loaded thread resumed from external rollout");
    assert_eq!(loaded.preview, preview);
    assert_eq!(loaded.path.as_ref(), Some(&external_rollout_path));
    assert_eq!(loaded.status, ThreadStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn thread_loaded_read_uses_loaded_thread_model_provider_override_when_rollout_metadata_is_missing()
-> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_multi_provider_config_toml(codex_home.path(), &server.uri())?;

    let preview = "provider override preview";
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        "2025-01-06T08-00-00",
        "2025-01-06T08:00:00Z",
        preview,
        /*model_provider*/ None,
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id.clone(),
            model_provider: Some("mock_provider".to_string()),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(thread.model_provider, "mock_provider");
    assert_eq!(thread.preview, preview);

    let read_id = mcp
        .send_thread_loaded_read_request(ThreadLoadedReadParams::default())
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadLoadedReadResponse { data, next_cursor } =
        to_response::<ThreadLoadedReadResponse>(read_resp)?;

    assert_eq!(next_cursor, None);
    let loaded = data
        .iter()
        .find(|candidate| candidate.id == conversation_id)
        .expect("thread/loaded/read should include loaded thread");
    assert_eq!(loaded.preview, preview);
    assert_eq!(loaded.model_provider, "mock_provider");
    assert_eq!(loaded.status, ThreadStatus::Idle);

    Ok(())
}

fn create_config_toml(codex_home: &Path, server_uri: &str) -> std::io::Result<()> {
    create_config_toml_with_approval_policy(codex_home, server_uri, "never")
}

fn create_config_toml_with_approval_policy(
    codex_home: &Path,
    server_uri: &str,
    approval_policy: &str,
) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "{approval_policy}"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}

fn create_multi_provider_config_toml(codex_home: &Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "target_provider"

[model_providers.target_provider]
name = "Target fallback provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[model_providers.mock_provider]
name = "Mock override provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}

fn create_config_toml_with_collaboration_modes(
    codex_home: &Path,
    server_uri: &str,
) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[features]
collaboration_modes = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}

fn create_config_toml_with_unified_exec(
    codex_home: &Path,
    server_uri: &str,
) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "danger-full-access"

model_provider = "mock_provider"

[features]
unified_exec = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}

fn create_background_exec_command_sse_response(call_id: &str) -> Result<String> {
    let cmd = if cfg!(windows) {
        r#"cmd.exe /d /c "ping -n 4 127.0.0.1 > nul""#
    } else {
        "/bin/sh -c 'sleep 3'"
    };
    let tool_call_arguments = serde_json::to_string(&serde_json::json!({
        "cmd": cmd,
        "yield_time_ms": 100
    }))?;
    Ok(responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_function_call(call_id, "exec_command", &tool_call_arguments),
        responses::ev_completed("resp-1"),
    ]))
}

async fn start_thread(mcp: &mut McpProcess) -> Result<String> {
    start_thread_in_cwd(mcp, if cfg!(windows) { r"C:\" } else { "/" }).await
}

async fn start_thread_in_cwd(mcp: &mut McpProcess, cwd: &str) -> Result<String> {
    let req_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.1".to_string()),
            cwd: Some(cwd.to_string()),
            ..Default::default()
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(req_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(resp)?;
    Ok(thread.id)
}
