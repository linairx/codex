use anyhow::Context;
use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::to_response;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadCloseParams;
use codex_app_server_protocol::ThreadCloseResponse;
use codex_app_server_protocol::ThreadCloseStatus;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_close_unloads_resident_thread_but_preserves_resident_mode() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let req_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
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

    let close_id = mcp
        .send_thread_close_request(ThreadCloseParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let close_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(close_id)),
    )
    .await??;
    let close = to_response::<ThreadCloseResponse>(close_resp)?;
    assert_eq!(close.status, ThreadCloseStatus::Closing);

    let closed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;
    let parsed: ServerNotification = closed_notif.try_into()?;
    let ServerNotification::ThreadClosed(payload) = parsed else {
        anyhow::bail!("expected thread/closed notification");
    };
    assert_eq!(payload.thread_id, thread.id);

    let status_changed = wait_for_thread_status_not_loaded(&mut mcp, &thread.id).await?;
    assert_eq!(status_changed.thread_id, thread.id);
    assert_eq!(status_changed.status, ThreadStatus::NotLoaded);

    let loaded_list_id = mcp
        .send_thread_loaded_list_request(ThreadLoadedListParams::default())
        .await?;
    let loaded_list_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(loaded_list_id)),
    )
    .await??;
    let ThreadLoadedListResponse { data, next_cursor } =
        to_response::<ThreadLoadedListResponse>(loaded_list_resp)?;
    assert_eq!(data, Vec::<String>::new());
    assert_eq!(next_cursor, None);

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
        thread: read_thread,
    } = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(read_thread.status, ThreadStatus::NotLoaded);

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse {
        thread: resumed, ..
    } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resumed.mode, ThreadMode::ResidentAssistant);
    assert_eq!(resumed.id, thread.id);

    Ok(())
}

#[tokio::test]
async fn thread_close_reports_not_loaded_once_thread_is_gone() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_id = start_thread(&mut mcp).await?;

    let close_id = mcp
        .send_thread_close_request(ThreadCloseParams {
            thread_id: thread_id.clone(),
        })
        .await?;
    let close_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(close_id)),
    )
    .await??;
    let close = to_response::<ThreadCloseResponse>(close_resp)?;
    assert_eq!(close.status, ThreadCloseStatus::Closing);

    let _: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/closed"),
    )
    .await??;
    let _ = wait_for_thread_status_not_loaded(&mut mcp, &thread_id).await?;

    let second_close_id = mcp
        .send_thread_close_request(ThreadCloseParams { thread_id })
        .await?;
    let second_close_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(second_close_id)),
    )
    .await??;
    let second_close = to_response::<ThreadCloseResponse>(second_close_resp)?;
    assert_eq!(second_close.status, ThreadCloseStatus::NotLoaded);

    Ok(())
}

async fn wait_for_thread_status_not_loaded(
    mcp: &mut McpProcess,
    thread_id: &str,
) -> Result<ThreadStatusChangedNotification> {
    loop {
        let status_changed_notif: JSONRPCNotification = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp.read_stream_until_notification_message("thread/status/changed"),
        )
        .await??;
        let status_changed_params = status_changed_notif
            .params
            .context("thread/status/changed params must be present")?;
        let status_changed: ThreadStatusChangedNotification =
            serde_json::from_value(status_changed_params)?;
        if status_changed.thread_id == thread_id && status_changed.status == ThreadStatus::NotLoaded
        {
            return Ok(status_changed);
        }
    }
}

fn create_config_toml(codex_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "danger-full-access"

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

async fn start_thread(mcp: &mut McpProcess) -> Result<String> {
    let req_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
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
