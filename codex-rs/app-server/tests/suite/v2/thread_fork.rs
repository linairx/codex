use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::create_fake_rollout_with_text_elements;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::rollout_path;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use codex_app_server_protocol::CommandAction;
use codex_app_server_protocol::CommandExecutionSource;
use codex_app_server_protocol::CommandExecutionStatus;
use codex_app_server_protocol::GuardianApprovalReview;
use codex_app_server_protocol::GuardianApprovalReviewAction;
use codex_app_server_protocol::GuardianApprovalReviewState;
use codex_app_server_protocol::GuardianApprovalReviewStatus;
use codex_app_server_protocol::GuardianRiskLevel;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::NetworkApprovalProtocol;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStartedNotification;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_login::AuthCredentialsStoreMode;
use codex_login::REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR;
use codex_protocol::ThreadId;
use codex_protocol::parse_command::ParsedCommand;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ExecCommandBeginEvent;
use codex_protocol::protocol::ExecCommandSource as CoreExecCommandSource;
use codex_protocol::protocol::GuardianAssessmentAction;
use codex_protocol::protocol::GuardianAssessmentEvent;
use codex_protocol::protocol::GuardianAssessmentStatus;
use codex_protocol::protocol::GuardianRiskLevel as CoreGuardianRiskLevel;
use codex_protocol::protocol::TurnStartedEvent;
use codex_state::StateRuntime;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::analytics::assert_basic_thread_initialized_event;
use super::analytics::enable_analytics_capture;
use super::analytics::thread_initialized_event;
use super::analytics::wait_for_analytics_payload;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_fork_creates_new_thread_and_emits_started() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let original_path = codex_home
        .path()
        .join("sessions")
        .join("2025")
        .join("01")
        .join("05")
        .join(format!(
            "rollout-2025-01-05T12-00-00-{conversation_id}.jsonl"
        ));
    assert!(
        original_path.exists(),
        "expected original rollout to exist at {}",
        original_path.display()
    );
    let original_contents = std::fs::read_to_string(&original_path)?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: conversation_id.clone(),
            ..Default::default()
        })
        .await?;
    let fork_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
    )
    .await??;
    let fork_result = fork_resp.result.clone();
    let ThreadForkResponse { thread, .. } = to_response::<ThreadForkResponse>(fork_resp)?;

    // Wire contract: thread title field is `name`, serialized as null when unset.
    let thread_json = fork_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/fork result.thread must be an object");
    assert_eq!(
        thread_json.get("name"),
        Some(&Value::Null),
        "forked threads do not inherit a name; expected `name: null`"
    );

    let after_contents = std::fs::read_to_string(&original_path)?;
    assert_eq!(
        after_contents, original_contents,
        "fork should not mutate the original rollout file"
    );

    assert_ne!(thread.id, conversation_id);
    assert_eq!(thread.forked_from_id, Some(conversation_id.clone()));
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.model_provider, "mock_provider");
    assert_eq!(thread.status, ThreadStatus::Idle);
    let thread_path = thread.path.clone().expect("thread path");
    assert!(thread_path.is_absolute());
    assert_ne!(thread_path, original_path);
    assert!(thread.cwd.is_absolute());
    assert_eq!(thread.source, SessionSource::VsCode);
    assert_eq!(thread.name, None);

    assert_eq!(
        thread.turns.len(),
        1,
        "expected forked thread to include one turn"
    );
    let turn = &thread.turns[0];
    assert_eq!(turn.status, TurnStatus::Interrupted);
    assert_eq!(turn.items.len(), 1, "expected user message item");
    match &turn.items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![UserInput::Text {
                    text: preview.to_string(),
                    text_elements: Vec::new(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }

    // A corresponding thread/started notification should arrive.
    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    let notif = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = timeout(remaining, mcp.read_next_message()).await??;
        let JSONRPCMessage::Notification(notif) = message else {
            continue;
        };
        if notif.method == "thread/status/changed" {
            let status_changed: ThreadStatusChangedNotification =
                serde_json::from_value(notif.params.expect("params must be present"))?;
            if status_changed.thread_id == thread.id {
                anyhow::bail!(
                    "thread/fork should introduce the thread without a preceding thread/status/changed"
                );
            }
            continue;
        }
        if notif.method == "thread/started" {
            break notif;
        }
    };
    let started_params = notif.params.clone().expect("params must be present");
    let started_thread_json = started_params
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/started params.thread must be an object");
    assert_eq!(
        started_thread_json.get("name"),
        Some(&Value::Null),
        "thread/started must serialize `name: null` when unset"
    );
    let started: ThreadStartedNotification =
        serde_json::from_value(notif.params.expect("params must be present"))?;
    assert_eq!(started.thread, thread);

    Ok(())
}

#[tokio::test]
async fn thread_fork_from_resident_thread_stays_interactive_after_restart() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let forked_thread_id = {
        let mut mcp = McpProcess::new(codex_home.path()).await?;
        timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

        let start_id = mcp
            .send_thread_start_request(ThreadStartParams {
                model: Some("mock-model".to_string()),
                resident: true,
                ..Default::default()
            })
            .await?;
        let start_resp: JSONRPCResponse = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
        )
        .await??;
        let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;
        assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

        let turn_req = mcp
            .send_turn_start_request(TurnStartParams {
                thread_id: thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "materialize resident thread".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            })
            .await?;
        let turn_resp: JSONRPCResponse = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
        )
        .await??;
        let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
        timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_notification_message("turn/completed"),
        )
        .await??;

        let fork_id = mcp
            .send_thread_fork_request(ThreadForkParams {
                thread_id: thread.id.clone(),
                ..Default::default()
            })
            .await?;
        let fork_resp: JSONRPCResponse = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
        )
        .await??;
        let ThreadForkResponse { thread, .. } = to_response::<ThreadForkResponse>(fork_resp)?;
        assert!(!thread.resident);
        assert_eq!(thread.mode, ThreadMode::Interactive);

        let state_db =
            StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into()).await?;
        let metadata = state_db
            .get_thread(ThreadId::from_string(&thread.id)?)
            .await?
            .expect("forked thread metadata");
        assert_eq!(metadata.mode, "interactive");

        thread.id
    };

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let state_db =
        StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into()).await?;
    let metadata = state_db
        .get_thread(ThreadId::from_string(&forked_thread_id)?)
        .await?
        .expect("forked thread metadata after restart");
    assert_eq!(metadata.mode, "interactive");

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: forked_thread_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let read: ThreadReadResponse = to_response::<ThreadReadResponse>(read_resp)?;
    assert!(!read.thread.resident);
    assert_eq!(read.thread.mode, ThreadMode::Interactive);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(50),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: None,
            cwd: None,
            search_term: None,
        })
        .await?;
    let list_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(list_id)),
    )
    .await??;
    let ThreadListResponse { data, .. } = to_response::<ThreadListResponse>(list_resp)?;
    let listed_thread = data
        .iter()
        .find(|listed_thread| listed_thread.id == forked_thread_id)
        .expect("thread/list should include the forked thread after restart");
    assert!(!listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::Interactive);

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: forked_thread_id,
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let resume: ThreadResumeResponse = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert!(!resume.thread.resident);
    assert_eq!(resume.thread.mode, ThreadMode::Interactive);

    Ok(())
}

#[tokio::test]
async fn resident_thread_fork_observes_workspace_changes() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.path().display().to_string()),
            resident: true,
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "materialize resident source thread".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: thread.id.clone(),
            resident: true,
            ..Default::default()
        })
        .await?;
    let fork_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
    )
    .await??;
    let ThreadForkResponse { thread: forked, .. } = to_response::<ThreadForkResponse>(fork_resp)?;
    assert!(forked.resident);
    assert_eq!(forked.mode, ThreadMode::ResidentAssistant);

    std::fs::write(workspace.path().join("fork-watched.txt"), "changed")?;

    let changed_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let read_id = mcp
                .send_thread_read_request(ThreadReadParams {
                    thread_id: forked.id.clone(),
                    include_turns: false,
                })
                .await?;
            let read_resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(read_id))
                .await?;
            let read: ThreadReadResponse = to_response::<ThreadReadResponse>(read_resp)?;
            if let ThreadStatus::Active { active_flags } = &read.thread.status
                && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
            {
                return Ok::<ThreadStatus, anyhow::Error>(read.thread.status);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await??;

    assert_eq!(
        changed_status,
        ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
        }
    );

    Ok(())
}

#[tokio::test]
async fn thread_fork_preserves_network_guardian_review_on_command_item() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let filename_ts = "2025-01-05T12-00-00";
    let meta_rfc3339 = "2025-01-05T12:00:00Z";
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        filename_ts,
        meta_rfc3339,
        "Saved user message",
        Vec::new(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let rollout_file_path = rollout_path(codex_home.path(), filename_ts, &conversation_id);
    let persisted_rollout = std::fs::read_to_string(&rollout_file_path)?;
    let turn_id = "network-review-turn";
    let appended_rollout = [
        json!({
            "timestamp": meta_rfc3339,
            "type": "event_msg",
            "payload": serde_json::to_value(EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: turn_id.to_string(),
                model_context_window: None,
                collaboration_mode_kind: Default::default(),
            }))?,
        })
        .to_string(),
        json!({
            "timestamp": meta_rfc3339,
            "type": "event_msg",
            "payload": serde_json::to_value(EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "exec-1".to_string(),
                process_id: None,
                turn_id: turn_id.to_string(),
                command: vec!["curl".to_string(), "https://api.example.com".to_string()],
                cwd: PathBuf::from("/tmp"),
                parsed_cmd: vec![ParsedCommand::Unknown {
                    cmd: "curl https://api.example.com".to_string(),
                }],
                source: CoreExecCommandSource::Agent,
                interaction_input: None,
            }))?,
        })
        .to_string(),
        json!({
            "timestamp": meta_rfc3339,
            "type": "event_msg",
            "payload": serde_json::to_value(EventMsg::GuardianAssessment(GuardianAssessmentEvent {
                id: "exec-1".to_string(),
                turn_id: turn_id.to_string(),
                status: GuardianAssessmentStatus::Approved,
                risk_score: Some(12),
                risk_level: Some(CoreGuardianRiskLevel::Low),
                rationale: Some("expected outbound request".to_string()),
                action: GuardianAssessmentAction::NetworkAccess {
                    target: "https://api.example.com:443".to_string(),
                    host: "api.example.com".to_string(),
                    protocol: codex_protocol::protocol::NetworkApprovalProtocol::Https,
                    port: 443,
                },
            }))?,
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(
        &rollout_file_path,
        format!("{persisted_rollout}{appended_rollout}\n"),
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: conversation_id,
            ..Default::default()
        })
        .await?;
    let fork_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
    )
    .await??;
    let ThreadForkResponse { thread, .. } = to_response::<ThreadForkResponse>(fork_resp)?;

    assert_eq!(thread.status, ThreadStatus::Idle);
    assert_eq!(thread.turns.len(), 2);
    assert_eq!(thread.turns[1].id, turn_id);
    assert_eq!(thread.turns[1].status, TurnStatus::Interrupted);
    assert_eq!(
        thread.turns[1].items,
        vec![ThreadItem::CommandExecution {
            id: "exec-1".to_string(),
            command: "curl https://api.example.com".to_string(),
            cwd: PathBuf::from("/tmp"),
            process_id: None,
            source: CommandExecutionSource::Agent,
            status: CommandExecutionStatus::InProgress,
            command_actions: vec![CommandAction::Unknown {
                command: "curl https://api.example.com".to_string(),
            }],
            aggregated_output: None,
            exit_code: None,
            duration_ms: None,
            guardian_review: Some(GuardianApprovalReviewState {
                review: GuardianApprovalReview {
                    status: GuardianApprovalReviewStatus::Approved,
                    risk_score: Some(12),
                    risk_level: Some(GuardianRiskLevel::Low),
                    rationale: Some("expected outbound request".to_string()),
                },
                action: GuardianApprovalReviewAction::NetworkAccess {
                    target: "https://api.example.com:443".to_string(),
                    host: "api.example.com".to_string(),
                    protocol: NetworkApprovalProtocol::Https,
                    port: 443,
                },
            }),
        }]
    );

    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    let notif = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = timeout(remaining, mcp.read_next_message()).await??;
        let JSONRPCMessage::Notification(notif) = message else {
            continue;
        };
        if notif.method == "thread/status/changed" {
            let status_changed: ThreadStatusChangedNotification =
                serde_json::from_value(notif.params.expect("params must be present"))?;
            if status_changed.thread_id == thread.id {
                anyhow::bail!(
                    "thread/fork should introduce the thread without a preceding thread/status/changed"
                );
            }
            continue;
        }
        if notif.method == "thread/started" {
            break notif;
        }
    };
    let started: ThreadStartedNotification =
        serde_json::from_value(notif.params.expect("params must be present"))?;
    assert_eq!(started.thread, thread);

    Ok(())
}

#[tokio::test]
async fn thread_fork_tracks_thread_initialized_analytics() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;

    let codex_home = TempDir::new()?;
    create_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        &server.uri(),
        &server.uri(),
        /*general_analytics_enabled*/ true,
    )?;
    enable_analytics_capture(&server, codex_home.path()).await?;

    let conversation_id = create_fake_rollout(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        "Saved user message",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: conversation_id,
            ..Default::default()
        })
        .await?;
    let fork_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
    )
    .await??;
    let ThreadForkResponse { thread, .. } = to_response::<ThreadForkResponse>(fork_resp)?;

    let payload = wait_for_analytics_payload(&server, DEFAULT_READ_TIMEOUT).await?;
    let event = thread_initialized_event(&payload)?;
    assert_basic_thread_initialized_event(event, &thread.id, "mock-model", "forked");
    Ok(())
}

#[tokio::test]
async fn thread_fork_rejects_unmaterialized_thread() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: thread.id,
            ..Default::default()
        })
        .await?;
    let fork_err: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(fork_id)),
    )
    .await??;
    assert!(
        fork_err
            .error
            .message
            .contains("no rollout found for thread id"),
        "unexpected fork error: {}",
        fork_err.error.message
    );

    Ok(())
}

#[tokio::test]
async fn thread_fork_surfaces_cloud_requirements_load_errors() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/backend-api/wham/config/requirements"))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("content-type", "text/html")
                .set_body_string("<html>nope</html>"),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "code": "refresh_token_invalidated" }
        })))
        .mount(&server)
        .await;

    let codex_home = TempDir::new()?;
    let model_server = create_mock_responses_server_repeating_assistant("Done").await;
    let chatgpt_base_url = format!("{}/backend-api", server.uri());
    create_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        &model_server.uri(),
        &chatgpt_base_url,
        /*general_analytics_enabled*/ false,
    )?;
    write_chatgpt_auth(
        codex_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .refresh_token("stale-refresh-token")
            .plan_type("business")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123")
            .account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let conversation_id = create_fake_rollout(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        "Saved user message",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let refresh_token_url = format!("{}/oauth/token", server.uri());
    let mut mcp = McpProcess::new_with_env(
        codex_home.path(),
        &[
            ("OPENAI_API_KEY", None),
            (
                REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR,
                Some(refresh_token_url.as_str()),
            ),
        ],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: conversation_id,
            ..Default::default()
        })
        .await?;
    let fork_err: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(fork_id)),
    )
    .await??;

    assert!(
        fork_err
            .error
            .message
            .contains("failed to load configuration"),
        "unexpected fork error: {}",
        fork_err.error.message
    );
    assert_eq!(
        fork_err.error.data,
        Some(json!({
            "reason": "cloudRequirements",
            "errorCode": "Auth",
            "action": "relogin",
            "statusCode": 401,
            "detail": "Your access token could not be refreshed because your refresh token was revoked. Please log out and sign in again.",
        }))
    );

    Ok(())
}

#[tokio::test]
async fn thread_fork_ephemeral_remains_pathless_and_omits_listing() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let fork_id = mcp
        .send_thread_fork_request(ThreadForkParams {
            thread_id: conversation_id.clone(),
            ephemeral: true,
            ..Default::default()
        })
        .await?;
    let fork_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(fork_id)),
    )
    .await??;
    let fork_result = fork_resp.result.clone();
    let ThreadForkResponse { thread, .. } = to_response::<ThreadForkResponse>(fork_resp)?;
    let fork_thread_id = thread.id.clone();

    assert!(
        thread.ephemeral,
        "ephemeral forks should be marked explicitly"
    );
    assert_eq!(
        thread.path, None,
        "ephemeral forks should not expose a path"
    );
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.status, ThreadStatus::Idle);
    assert_eq!(thread.name, None);
    assert_eq!(thread.turns.len(), 1, "expected copied fork history");

    let turn = &thread.turns[0];
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(turn.items.len(), 1, "expected user message item");
    match &turn.items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![UserInput::Text {
                    text: preview.to_string(),
                    text_elements: Vec::new(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }

    let thread_json = fork_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/fork result.thread must be an object");
    assert_eq!(
        thread_json.get("ephemeral").and_then(Value::as_bool),
        Some(true),
        "ephemeral forks should serialize `ephemeral: true`"
    );

    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    let notif = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = timeout(remaining, mcp.read_next_message()).await??;
        let JSONRPCMessage::Notification(notif) = message else {
            continue;
        };
        if notif.method == "thread/status/changed" {
            let status_changed: ThreadStatusChangedNotification =
                serde_json::from_value(notif.params.expect("params must be present"))?;
            if status_changed.thread_id == fork_thread_id {
                anyhow::bail!(
                    "thread/fork should introduce the thread without a preceding thread/status/changed"
                );
            }
            continue;
        }
        if notif.method == "thread/started" {
            break notif;
        }
    };
    let started_params = notif.params.clone().expect("params must be present");
    let started_thread_json = started_params
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/started params.thread must be an object");
    assert_eq!(
        started_thread_json
            .get("ephemeral")
            .and_then(Value::as_bool),
        Some(true),
        "thread/started should serialize `ephemeral: true` for ephemeral forks"
    );
    let started: ThreadStartedNotification =
        serde_json::from_value(notif.params.expect("params must be present"))?;
    assert_eq!(started.thread, thread);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(10),
            sort_key: None,
            model_providers: None,
            source_kinds: None,
            archived: None,
            cwd: None,
            search_term: None,
        })
        .await?;
    let list_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(list_id)),
    )
    .await??;
    let ThreadListResponse { data, .. } = to_response::<ThreadListResponse>(list_resp)?;
    assert!(
        data.iter().all(|candidate| candidate.id != fork_thread_id),
        "ephemeral forks should not appear in thread/list"
    );
    assert!(
        data.iter().any(|candidate| candidate.id == conversation_id),
        "persistent source thread should remain listed"
    );

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: fork_thread_id,
            input: vec![UserInput::Text {
                text: "continue".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

// Helper to create a config.toml pointing at the mock model server.
fn create_config_toml(codex_home: &Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
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

fn create_config_toml_with_chatgpt_base_url(
    codex_home: &Path,
    server_uri: &str,
    chatgpt_base_url: &str,
    general_analytics_enabled: bool,
) -> std::io::Result<()> {
    let general_analytics_toml = if general_analytics_enabled {
        "\ngeneral_analytics = true".to_string()
    } else {
        String::new()
    };
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
chatgpt_base_url = "{chatgpt_base_url}"

model_provider = "mock_provider"

[features]
{general_analytics_toml}

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
