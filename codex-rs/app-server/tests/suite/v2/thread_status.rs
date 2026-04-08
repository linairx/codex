use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::to_response;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput as V2UserInput;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_status_changed_emits_runtime_updates() -> Result<()> {
    let codex_home = TempDir::new()?;
    let responses = vec![create_final_assistant_message_sse_response("done")?];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp =
        McpProcess::new_with_env(codex_home.path(), &[("RUST_LOG", Some("info"))]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "collect status updates".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response(turn_start_resp)?;

    let mut saw_active_running = false;
    let mut saw_idle_after_turn = false;
    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = match timeout(remaining, mcp.read_next_message()).await {
            Ok(Ok(message)) => message,
            _ => break,
        };
        match message {
            JSONRPCMessage::Notification(JSONRPCNotification {
                method,
                params: Some(params),
            }) if method == "thread/status/changed" => {
                let notification: ThreadStatusChangedNotification = serde_json::from_value(params)?;
                if notification.thread_id != thread.id {
                    continue;
                }
                match notification.status {
                    ThreadStatus::Active { .. } => {
                        saw_active_running = true;
                    }
                    ThreadStatus::Idle => {
                        if saw_active_running {
                            saw_idle_after_turn = true;
                        }
                    }
                    ThreadStatus::SystemError => {
                        if saw_active_running {
                            saw_idle_after_turn = true;
                        }
                    }
                    ThreadStatus::NotLoaded => {
                        if saw_active_running {
                            saw_idle_after_turn = true;
                        }
                    }
                }
            }
            _ => {}
        }

        if saw_active_running && saw_idle_after_turn {
            break;
        }
    }

    assert!(
        saw_active_running,
        "expected running active flag in thread/status/changed notifications"
    );
    assert!(
        saw_idle_after_turn,
        "expected idle status after turn completion in thread/status/changed notifications"
    );
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test]
async fn thread_status_changed_can_be_opted_out() -> Result<()> {
    let codex_home = TempDir::new()?;
    let responses = vec![create_final_assistant_message_sse_response("done")?];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    let message = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.initialize_with_capabilities(
            ClientInfo {
                name: "codex_vscode".to_string(),
                title: Some("Codex VS Code Extension".to_string()),
                version: "0.1.0".to_string(),
            },
            Some(InitializeCapabilities {
                experimental_api: true,
                opt_out_notification_methods: Some(vec!["thread/status/changed".to_string()]),
            }),
        ),
    )
    .await??;
    let JSONRPCMessage::Response(_) = message else {
        anyhow::bail!("expected initialize response, got {message:?}");
    };

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id,
            input: vec![V2UserInput::Text {
                text: "run once".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response(turn_start_resp)?;

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let status_update = timeout(
        std::time::Duration::from_millis(500),
        mcp.read_stream_until_notification_message("thread/status/changed"),
    )
    .await;
    match status_update {
        Err(_) => {}
        Ok(Ok(notification)) => {
            anyhow::bail!(
                "thread/status/changed should be filtered by optOutNotificationMethods; got: {notification:?}"
            );
        }
        Ok(Err(err)) => {
            anyhow::bail!(
                "expected timeout waiting for filtered thread/status/changed, got: {err}"
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_status_changed_tracks_background_terminal_lifecycle() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let codex_home = TempDir::new()?;
    let responses = vec![
        create_background_exec_command_sse_response("uexec-bg")?,
        create_final_assistant_message_sse_response("done")?,
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml_with_unified_exec(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
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
    let _: TurnStartResponse = to_response(turn_start_resp)?;

    let mut saw_background_terminal_running = false;
    let mut saw_idle_after_background_terminal = false;
    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = match timeout(remaining, mcp.read_next_message()).await {
            Ok(Ok(message)) => message,
            _ => break,
        };
        match message {
            JSONRPCMessage::Notification(JSONRPCNotification {
                method,
                params: Some(params),
            }) if method == "thread/status/changed" => {
                let notification: ThreadStatusChangedNotification = serde_json::from_value(params)?;
                if notification.thread_id != thread.id {
                    continue;
                }
                match notification.status {
                    ThreadStatus::Active { active_flags } => {
                        if active_flags.contains(&ThreadActiveFlag::BackgroundTerminalRunning) {
                            saw_background_terminal_running = true;
                        }
                    }
                    ThreadStatus::Idle if saw_background_terminal_running => {
                        saw_idle_after_background_terminal = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        if saw_background_terminal_running && saw_idle_after_background_terminal {
            break;
        }
    }

    assert!(
        saw_background_terminal_running,
        "expected backgroundTerminalRunning in thread/status/changed notifications"
    );
    assert!(
        saw_idle_after_background_terminal,
        "expected idle status after background terminal exited"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_status_changed_tracks_workspace_changes() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let server = create_mock_responses_server_sequence_unchecked(Vec::new()).await;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.path().display().to_string()),
            resident: true,
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    std::fs::write(workspace.path().join("watched.txt"), "changed")?;

    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = match timeout(remaining, mcp.read_next_message()).await {
            Ok(Ok(message)) => message,
            _ => break,
        };
        let JSONRPCMessage::Notification(JSONRPCNotification {
            method,
            params: Some(params),
        }) = message
        else {
            continue;
        };
        if method != "thread/status/changed" {
            continue;
        }

        let notification: ThreadStatusChangedNotification = serde_json::from_value(params)?;
        if notification.thread_id != thread.id {
            continue;
        }
        if let ThreadStatus::Active { active_flags } = notification.status
            && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
        {
            return Ok(());
        }
    }

    anyhow::bail!("expected workspaceChanged in thread/status/changed notifications")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thread_status_changed_notification_does_not_repeat_thread_mode() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let server = create_mock_responses_server_sequence_unchecked(Vec::new()).await;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.path().display().to_string()),
            resident: true,
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;
    assert_eq!(thread.mode, ThreadMode::ResidentAssistant);

    std::fs::write(workspace.path().join("watched.txt"), "changed")?;

    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = match timeout(remaining, mcp.read_next_message()).await {
            Ok(Ok(message)) => message,
            _ => break,
        };
        let JSONRPCMessage::Notification(JSONRPCNotification {
            method,
            params: Some(params),
        }) = message
        else {
            continue;
        };
        if method != "thread/status/changed" {
            continue;
        }

        let notification: ThreadStatusChangedNotification = serde_json::from_value(params.clone())?;
        if notification.thread_id != thread.id {
            continue;
        }
        if let ThreadStatus::Active { active_flags } = &notification.status
            && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
        {
            let Value::Object(params) = params else {
                anyhow::bail!("expected thread/status/changed params object, got {params:?}");
            };
            assert_eq!(
                params.get("threadId"),
                Some(&Value::String(thread.id.clone()))
            );
            assert!(
                params.contains_key("status"),
                "expected thread/status/changed payload to include status: {params:?}"
            );
            assert!(
                !params.contains_key("mode"),
                "thread/status/changed should stay status-only and omit mode: {params:?}"
            );
            return Ok(());
        }
    }

    anyhow::bail!("expected workspaceChanged in thread/status/changed notifications")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workspace_changed_clears_after_next_turn() -> Result<()> {
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let server =
        create_mock_responses_server_sequence(vec![create_final_assistant_message_sse_response(
            "done",
        )?])
        .await;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.path().display().to_string()),
            resident: true,
            ..Default::default()
        })
        .await?;
    let thread_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_start_resp)?;

    std::fs::write(workspace.path().join("watched.txt"), "changed")?;

    let deadline = tokio::time::Instant::now() + DEFAULT_READ_TIMEOUT;
    let mut saw_workspace_changed = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let message = match timeout(remaining, mcp.read_next_message()).await {
            Ok(Ok(message)) => message,
            _ => break,
        };
        let JSONRPCMessage::Notification(JSONRPCNotification {
            method,
            params: Some(params),
        }) = message
        else {
            continue;
        };
        if method != "thread/status/changed" {
            continue;
        }

        let notification: ThreadStatusChangedNotification = serde_json::from_value(params)?;
        if notification.thread_id != thread.id {
            continue;
        }
        if let ThreadStatus::Active { active_flags } = notification.status
            && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
        {
            saw_workspace_changed = true;
            break;
        }
    }
    assert!(
        saw_workspace_changed,
        "expected initial workspaceChanged status"
    );

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "clear workspace flag".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response(turn_start_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse {
        thread: thread_read,
    } = to_response(read_resp)?;
    assert_eq!(thread_read.status, ThreadStatus::Idle);

    Ok(())
}

fn create_config_toml(codex_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
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
    codex_home: &std::path::Path,
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
collaboration_modes = true
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
    let tool_call_arguments = serde_json::to_string(&json!({
        "cmd": cmd,
        "yield_time_ms": 100
    }))?;
    Ok(responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_function_call(call_id, "exec_command", &tool_call_arguments),
        responses::ev_completed("resp-1"),
    ]))
}
