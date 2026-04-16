use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::rollout_path;
use app_test_support::to_response;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::interactive_thread_source_kinds;
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
