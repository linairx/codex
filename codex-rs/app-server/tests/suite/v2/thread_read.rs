use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::create_fake_rollout_with_text_elements;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::rollout_path;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadNameUpdatedNotification;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadSetNameResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use codex_core::ARCHIVED_SESSIONS_SUBDIR;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource as ProtocolSessionSource;
use codex_protocol::user_input::ByteRange;
use codex_protocol::user_input::TextElement;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_read_returns_summary_without_turns() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let text_elements = [TextElement::new(
        ByteRange { start: 0, end: 5 },
        Some("<note>".into()),
    )];
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        text_elements
            .iter()
            .map(|elem| serde_json::to_value(elem).expect("serialize text element"))
            .collect(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.id, conversation_id);
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.model_provider, "mock_provider");
    assert!(!thread.ephemeral, "stored rollouts should not be ephemeral");
    assert!(thread.path.as_ref().expect("thread path").is_absolute());
    assert_eq!(thread.cwd, PathBuf::from("/"));
    assert_eq!(thread.cli_version, "0.0.0");
    assert_eq!(thread.source, SessionSource::Cli);
    assert_eq!(thread.git_info, None);
    assert_eq!(thread.turns.len(), 0);
    assert_eq!(thread.status, ThreadStatus::NotLoaded);

    Ok(())
}

#[tokio::test]
async fn stored_rollout_thread_read_and_list_preserve_rollout_summary_and_sqlite_mode() -> Result<()>
{
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Stored resident summary";
    let filename_ts = "2025-01-05T12-00-00";
    let timestamp_rfc3339 = "2025-01-05T12:00:00Z";
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        filename_ts,
        timestamp_rfc3339,
        preview,
        vec![],
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let rollout_path = rollout_path(codex_home.path(), filename_ts, &conversation_id);
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let state_db =
        StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into()).await?;
    let mut metadata = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path.clone(),
        Utc::now(),
        ProtocolSessionSource::Cli,
    );
    metadata.model_provider = Some("mock_provider".to_string());
    metadata.cwd = PathBuf::from("/");
    metadata.cli_version = Some("0.0.0".to_string());
    let inserted = state_db
        .insert_thread_if_absent(&metadata.build("mock_provider"))
        .await?;
    let updated = state_db
        .set_thread_mode(metadata.id, "residentAssistant")
        .await?;
    assert!(
        inserted || updated,
        "thread metadata setup should insert or update the stored rollout row"
    );
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
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

    assert_eq!(read_thread.id, conversation_id);
    assert_eq!(read_thread.preview, preview);
    assert_eq!(read_thread.path, Some(rollout_path.clone()));
    assert_eq!(read_thread.status, ThreadStatus::NotLoaded);
    assert!(read_thread.resident);
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);

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
        .find(|listed_thread| listed_thread.id == conversation_id)
        .expect("thread/list should include the stored resident rollout thread");
    assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

    Ok(())
}

#[tokio::test]
async fn archived_thread_read_and_list_reconcile_missing_summary_for_existing_sqlite_row()
-> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Archived resident summary";
    let filename_ts = "2025-01-05T12-00-00";
    let timestamp_rfc3339 = "2025-01-05T12:00:00Z";
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        filename_ts,
        timestamp_rfc3339,
        preview,
        vec![],
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let archived_dir = codex_home.path().join(ARCHIVED_SESSIONS_SUBDIR);
    fs::create_dir_all(&archived_dir)?;
    let rollout_path = rollout_path(codex_home.path(), filename_ts, &conversation_id);
    let archived_path = archived_dir.join(
        rollout_path
            .file_name()
            .expect("archived rollout should have a file name"),
    );
    fs::rename(&rollout_path, &archived_path)?;

    let thread_id = ThreadId::from_string(&conversation_id)?;
    let state_db =
        StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into()).await?;
    let mut metadata = ThreadMetadataBuilder::new(
        thread_id,
        archived_path.clone(),
        Utc::now(),
        ProtocolSessionSource::Cli,
    );
    metadata.model_provider = Some("mock_provider".to_string());
    metadata.cwd = PathBuf::from("/");
    metadata.cli_version = Some("0.0.0".to_string());
    let inserted = state_db
        .insert_thread_if_absent(&metadata.build("mock_provider"))
        .await?;
    assert!(
        inserted,
        "test setup should insert an incomplete archived sqlite row"
    );
    let updated_mode = state_db
        .set_thread_mode(thread_id, "residentAssistant")
        .await?;
    assert!(
        updated_mode,
        "test setup should preserve resident mode in sqlite"
    );
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
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

    assert_eq!(read_thread.id, conversation_id);
    assert_eq!(read_thread.preview, preview);
    assert_eq!(read_thread.path, Some(archived_path.clone()));
    assert_eq!(read_thread.status, ThreadStatus::NotLoaded);
    assert!(read_thread.resident);
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(50),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: Some(true),
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
        .into_iter()
        .find(|listed_thread| listed_thread.id == conversation_id)
        .expect("thread/list archived=true should include repaired resident thread");
    assert_eq!(listed_thread.preview, preview);
    assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

    let persisted = state_db
        .get_thread(thread_id)
        .await?
        .expect("thread metadata should exist after archived read repair");
    assert_eq!(persisted.first_user_message.as_deref(), Some(preview));
    assert!(persisted.archived_at.is_some());

    Ok(())
}

#[tokio::test]
async fn thread_read_can_include_turns() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let text_elements = vec![TextElement::new(
        ByteRange { start: 0, end: 5 },
        Some("<note>".into()),
    )];
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        text_elements
            .iter()
            .map(|elem| serde_json::to_value(elem).expect("serialize text element"))
            .collect(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
            include_turns: true,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.turns.len(), 1);
    let turn = &thread.turns[0];
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(turn.items.len(), 1, "expected user message item");
    match &turn.items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![UserInput::Text {
                    text: preview.to_string(),
                    text_elements: text_elements.clone().into_iter().map(Into::into).collect(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }
    assert_eq!(thread.status, ThreadStatus::NotLoaded);

    Ok(())
}

#[tokio::test]
async fn thread_read_returns_forked_from_id_for_forked_threads() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        "Saved user message",
        vec![],
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

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
    let ThreadForkResponse { thread: forked, .. } = to_response::<ThreadForkResponse>(fork_resp)?;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: forked.id,
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.forked_from_id, Some(conversation_id));

    Ok(())
}

#[tokio::test]
async fn thread_read_loaded_thread_returns_precomputed_path_before_materialization() -> Result<()> {
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
    let thread_path = thread.path.clone().expect("thread path");
    assert!(
        !thread_path.exists(),
        "fresh thread rollout should not be materialized yet"
    );

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
    let ThreadReadResponse { thread: read } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(read.id, thread.id);
    assert_eq!(read.path, Some(thread_path));
    assert!(read.preview.is_empty());
    assert_eq!(read.turns.len(), 0);
    assert_eq!(read.status, ThreadStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn thread_read_preserves_preview_for_loaded_thread_resumed_from_external_rollout()
-> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let external_home = TempDir::new()?;
    let preview = "external loaded preview";
    let conversation_id = create_fake_rollout_with_text_elements(
        external_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        vec![],
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let external_rollout_path = rollout_path(
        external_home.path(),
        "2025-01-05T12-00-00",
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
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.id, conversation_id);
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.path.as_ref(), Some(&external_rollout_path));
    assert_eq!(thread.status, ThreadStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn thread_read_uses_loaded_thread_model_provider_override_when_rollout_metadata_is_missing()
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
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.id, conversation_id);
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.model_provider, "mock_provider");
    assert_eq!(thread.status, ThreadStatus::Idle);

    Ok(())
}

#[tokio::test]
async fn thread_name_set_is_reflected_in_read_list_and_resume() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let conversation_id = create_fake_rollout_with_text_elements(
        codex_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        vec![],
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    // Set a user-facing thread title.
    let new_name = "My renamed thread";
    let set_id = mcp
        .send_thread_set_name_request(ThreadSetNameParams {
            thread_id: conversation_id.clone(),
            name: new_name.to_string(),
        })
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let _: ThreadSetNameResponse = to_response::<ThreadSetNameResponse>(set_resp)?;
    let notification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/name/updated"),
    )
    .await??;
    let notification: ThreadNameUpdatedNotification =
        serde_json::from_value(notification.params.expect("thread/name/updated params"))?;
    assert_eq!(notification.thread_id, conversation_id);
    assert_eq!(notification.thread_name.as_deref(), Some(new_name));

    // Read should now surface `thread.name`, and the wire payload must include `name`.
    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: conversation_id.clone(),
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let read_result = read_resp.result.clone();
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(thread.id, conversation_id);
    assert_eq!(thread.name.as_deref(), Some(new_name));
    let thread_json = read_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/read result.thread must be an object");
    assert_eq!(
        thread_json.get("name").and_then(Value::as_str),
        Some(new_name),
        "thread/read must serialize `thread.name` on the wire"
    );
    assert_eq!(
        thread_json.get("ephemeral").and_then(Value::as_bool),
        Some(false),
        "thread/read must serialize `thread.ephemeral` on the wire"
    );

    // List should also surface the name.
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
    let list_result = list_resp.result.clone();
    let ThreadListResponse { data, .. } = to_response::<ThreadListResponse>(list_resp)?;
    let listed = data
        .iter()
        .find(|t| t.id == conversation_id)
        .expect("thread/list should include the created thread");
    assert_eq!(listed.name.as_deref(), Some(new_name));
    let listed_json = list_result
        .get("data")
        .and_then(Value::as_array)
        .expect("thread/list result.data must be an array")
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(&conversation_id))
        .and_then(Value::as_object)
        .expect("thread/list should include the created thread as an object");
    assert_eq!(
        listed_json.get("name").and_then(Value::as_str),
        Some(new_name),
        "thread/list must serialize `thread.name` on the wire"
    );
    assert_eq!(
        listed_json.get("ephemeral").and_then(Value::as_bool),
        Some(false),
        "thread/list must serialize `thread.ephemeral` on the wire"
    );

    // Resume should also surface the name.
    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let resume_result = resume_resp.result.clone();
    let ThreadResumeResponse {
        thread: resumed, ..
    } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resumed.id, conversation_id);
    assert_eq!(resumed.name.as_deref(), Some(new_name));
    let resumed_json = resume_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/resume result.thread must be an object");
    assert_eq!(
        resumed_json.get("name").and_then(Value::as_str),
        Some(new_name),
        "thread/resume must serialize `thread.name` on the wire"
    );
    assert_eq!(
        resumed_json.get("ephemeral").and_then(Value::as_bool),
        Some(false),
        "thread/resume must serialize `thread.ephemeral` on the wire"
    );

    Ok(())
}

#[tokio::test]
async fn resident_thread_name_is_reflected_across_loaded_only_read_surfaces() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

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

    let new_name = "Loaded only resident thread";
    let set_id = mcp
        .send_thread_set_name_request(ThreadSetNameParams {
            thread_id: thread.id.clone(),
            name: new_name.to_string(),
        })
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let _: ThreadSetNameResponse = to_response::<ThreadSetNameResponse>(set_resp)?;
    let notification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/name/updated"),
    )
    .await??;
    let notification: ThreadNameUpdatedNotification =
        serde_json::from_value(notification.params.expect("thread/name/updated params"))?;
    assert_eq!(notification.thread_id, thread.id);
    assert_eq!(notification.thread_name.as_deref(), Some(new_name));

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
    assert_eq!(read_thread.name.as_deref(), Some(new_name));
    assert!(read_thread.resident);
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(read_thread.status, ThreadStatus::Idle);

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
        .expect("thread/loaded/read should include the loaded-only resident thread");
    assert_eq!(loaded_thread.name.as_deref(), Some(new_name));
    assert!(loaded_thread.resident);
    assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(loaded_thread.status, ThreadStatus::Idle);

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
    let list_result = list_resp.result.clone();
    let ThreadListResponse { data, .. } = to_response::<ThreadListResponse>(list_resp)?;
    let listed = data
        .iter()
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include the loaded-only resident thread");
    assert_eq!(listed.name.as_deref(), Some(new_name));
    assert!(listed.resident);
    assert_eq!(listed.mode, ThreadMode::ResidentAssistant);
    assert_eq!(listed.status, ThreadStatus::Idle);
    let listed_json = list_result
        .get("data")
        .and_then(Value::as_array)
        .expect("thread/list result.data must be an array")
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(&thread.id))
        .and_then(Value::as_object)
        .expect("thread/list should include the loaded-only resident thread as an object");
    assert_eq!(
        listed_json.get("name").and_then(Value::as_str),
        Some(new_name),
        "thread/list must serialize `thread.name` for loaded-only resident thread entries"
    );
    assert_eq!(
        listed_json.get("mode").and_then(Value::as_str),
        Some("residentAssistant"),
        "thread/list must serialize `thread.mode` for loaded-only resident thread entries"
    );

    Ok(())
}

#[tokio::test]
async fn resident_thread_mode_is_consistent_across_read_surfaces() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

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

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "materialize".to_string(),
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
    assert!(read_thread.resident);
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);

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
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include the resident thread");
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

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
    assert!(loaded_thread.resident);
    assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);

    Ok(())
}

#[tokio::test]
async fn resident_thread_workspace_changes_are_visible_across_read_surfaces() -> Result<()> {
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

    std::fs::write(workspace.path().join("watched.txt"), "changed")?;

    let changed_status = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let read_id = mcp
                .send_thread_read_request(ThreadReadParams {
                    thread_id: thread.id.clone(),
                    include_turns: false,
                })
                .await?;
            let read_resp: JSONRPCResponse = mcp
                .read_stream_until_response_message(RequestId::Integer(read_id))
                .await?;
            let ThreadReadResponse {
                thread: read_thread,
            } = to_response::<ThreadReadResponse>(read_resp)?;
            if let ThreadStatus::Active { active_flags } = &read_thread.status
                && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
            {
                return Ok::<ThreadStatus, anyhow::Error>(read_thread.status);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
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
    assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(loaded_thread.status, changed_status);

    Ok(())
}

#[tokio::test]
async fn thread_read_include_turns_rejects_unmaterialized_loaded_thread() -> Result<()> {
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
    let thread_path = thread.path.clone().expect("thread path");
    assert!(
        !thread_path.exists(),
        "fresh thread rollout should not be materialized yet"
    );

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id.clone(),
            include_turns: true,
        })
        .await?;
    let read_err: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(read_id)),
    )
    .await??;

    assert!(
        read_err
            .error
            .message
            .contains("includeTurns is unavailable before first user message"),
        "unexpected error: {}",
        read_err.error.message
    );

    Ok(())
}

#[tokio::test]
async fn thread_read_reports_system_error_idle_flag_after_failed_turn() -> Result<()> {
    let server = responses::start_mock_server().await;
    let _response_mock = responses::mount_sse_once(
        &server,
        responses::sse_failed("resp-1", "server_error", "simulated failure"),
    )
    .await;
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

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "fail this turn".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_start_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_start_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(turn_start_response)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("error"),
    )
    .await??;

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: thread.id,
            include_turns: false,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse { thread } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(thread.status, ThreadStatus::SystemError,);

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
