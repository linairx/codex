use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadArchiveResponse;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadSetNameResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadUnarchiveParams;
use codex_app_server_protocol::ThreadUnarchiveResponse;
use codex_app_server_protocol::ThreadUnarchivedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_core::find_archived_thread_path_by_id_str;
use codex_core::find_thread_path_by_id_str;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource as ProtocolSessionSource;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::fs::FileTimes;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[tokio::test]
async fn thread_unarchive_moves_rollout_back_into_sessions_directory() -> Result<()> {
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

    let rollout_path = thread.path.clone().expect("thread path");

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "materialize".to_string(),
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
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let found_rollout_path = find_thread_path_by_id_str(codex_home.path(), &thread.id)
        .await?
        .expect("expected rollout path for thread id to exist");
    assert_paths_match_on_disk(&found_rollout_path, &rollout_path)?;

    let archive_id = mcp
        .send_thread_archive_request(ThreadArchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let archive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(archive_id)),
    )
    .await??;
    let _: ThreadArchiveResponse = to_response::<ThreadArchiveResponse>(archive_resp)?;

    let archived_path = find_archived_thread_path_by_id_str(codex_home.path(), &thread.id)
        .await?
        .expect("expected archived rollout path for thread id to exist");
    let archived_path_display = archived_path.display();
    assert!(
        archived_path.exists(),
        "expected {archived_path_display} to exist"
    );
    let old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    let old_timestamp = old_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("old timestamp")
        .as_secs() as i64;
    let times = FileTimes::new().set_modified(old_time);
    OpenOptions::new()
        .append(true)
        .open(&archived_path)?
        .set_times(times)?;

    let unarchive_id = mcp
        .send_thread_unarchive_request(ThreadUnarchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let unarchive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unarchive_id)),
    )
    .await??;
    let unarchive_result = unarchive_resp.result.clone();
    let ThreadUnarchiveResponse {
        thread: unarchived_thread,
    } = to_response::<ThreadUnarchiveResponse>(unarchive_resp)?;
    let unarchive_notification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/unarchived"),
    )
    .await??;
    let unarchived_notification: ThreadUnarchivedNotification = serde_json::from_value(
        unarchive_notification
            .params
            .expect("thread/unarchived notification params"),
    )?;
    assert_eq!(unarchived_notification.thread_id, thread.id);
    assert!(
        unarchived_thread.updated_at > old_timestamp,
        "expected updated_at to be bumped on unarchive"
    );
    assert_eq!(unarchived_thread.status, ThreadStatus::NotLoaded);

    // Wire contract: thread title field is `name`, serialized as null when unset.
    let thread_json = unarchive_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/unarchive result.thread must be an object");
    assert_eq!(unarchived_thread.name, None);
    assert_eq!(
        thread_json.get("name"),
        Some(&Value::Null),
        "thread/unarchive must serialize `name: null` when unset"
    );

    let rollout_path_display = rollout_path.display();
    assert!(
        rollout_path.exists(),
        "expected rollout path {rollout_path_display} to be restored"
    );
    assert!(
        !archived_path.exists(),
        "expected archived rollout path {archived_path_display} to be moved"
    );

    Ok(())
}

#[tokio::test]
async fn resident_thread_unarchive_preserves_resident_mode() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
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

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "materialize resident thread".to_string(),
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
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let archive_id = mcp
        .send_thread_archive_request(ThreadArchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let archive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(archive_id)),
    )
    .await??;
    let _: ThreadArchiveResponse = to_response::<ThreadArchiveResponse>(archive_resp)?;

    let unarchive_id = mcp
        .send_thread_unarchive_request(ThreadUnarchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let unarchive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unarchive_id)),
    )
    .await??;
    let ThreadUnarchiveResponse {
        thread: unarchived_thread,
    } = to_response::<ThreadUnarchiveResponse>(unarchive_resp)?;
    assert!(unarchived_thread.resident);
    assert_eq!(unarchived_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(unarchived_thread.status, ThreadStatus::NotLoaded);

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
    assert_eq!(read_thread.status, ThreadStatus::NotLoaded);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(50),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: Some(false),
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
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include unarchived resident thread");
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);

    Ok(())
}

#[tokio::test]
async fn named_resident_thread_unarchive_preserves_name_and_mode() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
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

    let thread_name = "Named resident unarchive thread";
    let set_name_id = mcp
        .send_thread_set_name_request(ThreadSetNameParams {
            thread_id: thread.id.clone(),
            name: thread_name.to_string(),
        })
        .await?;
    let set_name_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_name_id)),
    )
    .await??;
    let _: ThreadSetNameResponse = to_response::<ThreadSetNameResponse>(set_name_resp)?;

    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "materialize named resident thread".to_string(),
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
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let archive_id = mcp
        .send_thread_archive_request(ThreadArchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let archive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(archive_id)),
    )
    .await??;
    let _: ThreadArchiveResponse = to_response::<ThreadArchiveResponse>(archive_resp)?;

    let unarchive_id = mcp
        .send_thread_unarchive_request(ThreadUnarchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let unarchive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unarchive_id)),
    )
    .await??;
    let unarchive_result = unarchive_resp.result.clone();
    let ThreadUnarchiveResponse {
        thread: unarchived_thread,
    } = to_response::<ThreadUnarchiveResponse>(unarchive_resp)?;
    assert!(unarchived_thread.resident);
    assert_eq!(unarchived_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(unarchived_thread.status, ThreadStatus::NotLoaded);
    assert_eq!(unarchived_thread.name.as_deref(), Some(thread_name));
    let notification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("thread/unarchived"),
    )
    .await??;
    let notification_params = notification
        .params
        .as_ref()
        .and_then(Value::as_object)
        .expect("thread/unarchived params should be an object");
    assert_eq!(
        notification_params.get("threadId").and_then(Value::as_str),
        Some(thread.id.as_str())
    );
    assert_eq!(
        notification_params.len(),
        1,
        "thread/unarchived should stay identity-only even when restored thread summary has name + mode"
    );
    let thread_json = unarchive_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/unarchive result.thread must be an object");
    assert_eq!(
        thread_json.get("name").and_then(Value::as_str),
        Some(thread_name),
        "thread/unarchive response must carry the authoritative restored thread name"
    );
    assert_eq!(
        thread_json.get("mode").and_then(Value::as_str),
        Some("residentAssistant"),
        "thread/unarchive response must carry the authoritative restored thread mode"
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
    let ThreadReadResponse {
        thread: read_thread,
    } = to_response::<ThreadReadResponse>(read_resp)?;
    assert!(read_thread.resident);
    assert_eq!(read_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(read_thread.status, ThreadStatus::NotLoaded);
    assert_eq!(read_thread.name.as_deref(), Some(thread_name));

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(50),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: Some(false),
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
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include unarchived named resident thread");
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(listed_thread.status, ThreadStatus::NotLoaded);
    assert_eq!(listed_thread.name.as_deref(), Some(thread_name));

    Ok(())
}

#[tokio::test]
async fn resident_thread_unarchive_reconciles_missing_summary_for_existing_sqlite_row() -> Result<()>
{
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;
    let state_db = init_state_db(codex_home.path()).await?;

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

    let preview = "materialize resident thread";
    let turn_start_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: preview.to_string(),
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
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let archive_id = mcp
        .send_thread_archive_request(ThreadArchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let archive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(archive_id)),
    )
    .await??;
    let _: ThreadArchiveResponse = to_response::<ThreadArchiveResponse>(archive_resp)?;

    let thread_uuid = ThreadId::from_string(&thread.id)?;
    let archived_path = find_archived_thread_path_by_id_str(codex_home.path(), &thread.id)
        .await?
        .expect("archived rollout should exist");
    assert_eq!(state_db.delete_thread(thread_uuid).await?, 1);
    let mut metadata = ThreadMetadataBuilder::new(
        thread_uuid,
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
        "test setup should insert an incomplete sqlite row"
    );
    let updated_mode = state_db
        .set_thread_mode(thread_uuid, "residentAssistant")
        .await?;
    assert!(
        updated_mode,
        "test setup should preserve resident mode in sqlite"
    );

    let unarchive_id = mcp
        .send_thread_unarchive_request(ThreadUnarchiveParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let unarchive_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(unarchive_id)),
    )
    .await??;
    let ThreadUnarchiveResponse {
        thread: unarchived_thread,
    } = to_response::<ThreadUnarchiveResponse>(unarchive_resp)?;
    assert!(unarchived_thread.resident);
    assert_eq!(unarchived_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(unarchived_thread.status, ThreadStatus::NotLoaded);
    assert_eq!(unarchived_thread.preview, preview);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(50),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: Some(false),
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
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include unarchived resident thread");
    assert_eq!(listed_thread.preview, preview);
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);

    let persisted = state_db
        .get_thread(thread_uuid)
        .await?
        .expect("thread metadata should exist after unarchive");
    assert_eq!(persisted.first_user_message.as_deref(), Some(preview));
    assert!(persisted.archived_at.is_none());

    Ok(())
}

async fn init_state_db(codex_home: &Path) -> Result<Arc<StateRuntime>> {
    let state_db = StateRuntime::init(codex_home.to_path_buf(), "mock_provider".into()).await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;
    Ok(state_db)
}

fn create_config_toml(codex_home: &Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(config_toml, config_contents(server_uri))
}

fn config_contents(server_uri: &str) -> String {
    format!(
        r#"model = "mock-model"
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
    )
}

fn assert_paths_match_on_disk(actual: &Path, expected: &Path) -> std::io::Result<()> {
    let actual = actual.canonicalize()?;
    let expected = expected.canonicalize()?;
    assert_eq!(actual, expected);
    Ok(())
}
