use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadRollbackParams;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadSetNameResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput as V2UserInput;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource as ProtocolSessionSource;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn thread_rollback_drops_last_turns_and_persists_to_rollout() -> Result<()> {
    // Three Codex turns hit the mock model (session start + two turn/start calls).
    let responses = vec![
        create_final_assistant_message_sse_response("Done")?,
        create_final_assistant_message_sse_response("Done")?,
        create_final_assistant_message_sse_response("Done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;

    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    // Start a thread.
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

    // Two turns.
    let first_text = "First";
    let turn1_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: first_text.to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _turn1_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn1_id)),
    )
    .await??;
    let _completed1 = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let turn2_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "Second".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _turn2_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn2_id)),
    )
    .await??;
    let _completed2 = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    // Roll back the last turn.
    let rollback_id = mcp
        .send_thread_rollback_request(ThreadRollbackParams {
            thread_id: thread.id.clone(),
            num_turns: 1,
        })
        .await?;
    let rollback_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(rollback_id)),
    )
    .await??;
    let rollback_result = rollback_resp.result.clone();
    let ThreadRollbackResponse {
        thread: rolled_back_thread,
    } = to_response::<ThreadRollbackResponse>(rollback_resp)?;

    // Wire contract: thread title field is `name`, serialized as null when unset.
    let thread_json = rollback_result
        .get("thread")
        .and_then(Value::as_object)
        .expect("thread/rollback result.thread must be an object");
    assert_eq!(rolled_back_thread.name, None);
    assert_eq!(
        thread_json.get("name"),
        Some(&Value::Null),
        "thread/rollback must serialize `name: null` when unset"
    );

    assert_eq!(rolled_back_thread.turns.len(), 1);
    assert_eq!(rolled_back_thread.status, ThreadStatus::Idle);
    assert_eq!(rolled_back_thread.turns[0].items.len(), 2);
    match &rolled_back_thread.turns[0].items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![V2UserInput::Text {
                    text: first_text.to_string(),
                    text_elements: Vec::new(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }

    // Resume and confirm the history is pruned.
    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id,
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;

    assert_eq!(thread.turns.len(), 1);
    assert_eq!(thread.status, ThreadStatus::Idle);
    assert_eq!(thread.turns[0].items.len(), 2);
    match &thread.turns[0].items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![V2UserInput::Text {
                    text: first_text.to_string(),
                    text_elements: Vec::new(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn resident_thread_rollback_preserves_mode_for_follow_up_read_and_list() -> Result<()> {
    let responses = vec![
        create_final_assistant_message_sse_response("Done")?,
        create_final_assistant_message_sse_response("Done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;

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
            input: vec![V2UserInput::Text {
                text: "rollback resident turn".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: JSONRPCResponse = turn_resp;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let rollback_id = mcp
        .send_thread_rollback_request(ThreadRollbackParams {
            thread_id: thread.id.clone(),
            num_turns: 1,
        })
        .await?;
    let rollback_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(rollback_id)),
    )
    .await??;
    let rollback: ThreadRollbackResponse = to_response::<ThreadRollbackResponse>(rollback_resp)?;

    assert_eq!(rollback.thread.id, thread.id);
    assert_eq!(rollback.thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(rollback.thread.status, ThreadStatus::Idle);
    assert!(rollback.thread.resident);

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
    let read: ThreadReadResponse = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(read.thread.status, ThreadStatus::Idle);
    assert!(read.thread.resident);

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(20),
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
    let listed_thread = data
        .iter()
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include rolled back resident thread");
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(listed_thread.status, ThreadStatus::Idle);
    assert!(listed_thread.resident);

    Ok(())
}

#[tokio::test]
async fn resident_thread_rollback_reconciles_missing_summary_for_existing_sqlite_row() -> Result<()>
{
    let responses = vec![
        create_final_assistant_message_sse_response("Done")?,
        create_final_assistant_message_sse_response("Done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;

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

    let preview = "rollback repaired preview";
    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: preview.to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: JSONRPCResponse = turn_resp;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let thread_id = ThreadId::from_string(&thread.id)?;
    let rollout_path = thread
        .path
        .clone()
        .expect("thread/start should expose a persisted rollout path");
    let state_db =
        StateRuntime::init(codex_home.path().to_path_buf(), "mock_provider".into()).await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;
    let mut metadata = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path,
        Utc::now(),
        ProtocolSessionSource::Cli,
    );
    metadata.mode = "residentAssistant".to_string();
    metadata.model_provider = Some("mock_provider".to_string());
    metadata.cwd = PathBuf::from("/");
    metadata.cli_version = Some("0.0.0".to_string());
    state_db
        .upsert_thread(&metadata.build("mock_provider"))
        .await?;

    let rollback_id = mcp
        .send_thread_rollback_request(ThreadRollbackParams {
            thread_id: thread.id.clone(),
            num_turns: 1,
        })
        .await?;
    let rollback_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(rollback_id)),
    )
    .await??;
    let rollback: ThreadRollbackResponse = to_response::<ThreadRollbackResponse>(rollback_resp)?;

    assert_eq!(rollback.thread.id, thread.id);
    assert_eq!(rollback.thread.preview, preview);
    assert_eq!(rollback.thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(rollback.thread.status, ThreadStatus::Idle);
    assert!(rollback.thread.resident);

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
    let read: ThreadReadResponse = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(read.thread.preview, preview);
    assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
    assert!(read.thread.resident);

    let listed_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(20),
            sort_key: None,
            model_providers: None,
            source_kinds: None,
            archived: None,
            cwd: None,
            search_term: None,
        })
        .await?;
    let listed_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(listed_id)),
    )
    .await??;
    let listed: ThreadListResponse = to_response::<ThreadListResponse>(listed_resp)?;
    let listed_thread = listed
        .data
        .iter()
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include rolled back resident thread");
    assert_eq!(listed_thread.preview, preview);
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
    assert!(listed_thread.resident);

    let persisted = state_db
        .get_thread(thread_id)
        .await?
        .expect("thread metadata should exist after rollback repair");
    assert_eq!(persisted.first_user_message.as_deref(), Some(preview));

    Ok(())
}

#[tokio::test]
async fn named_resident_thread_rollback_preserves_name_and_mode_through_resume() -> Result<()> {
    let responses = vec![
        create_final_assistant_message_sse_response("Done")?,
        create_final_assistant_message_sse_response("Done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;

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

    let thread_name = "Named resident rollback thread";
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

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: "rollback named resident turn".to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    let _: JSONRPCResponse = turn_resp;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let rollback_id = mcp
        .send_thread_rollback_request(ThreadRollbackParams {
            thread_id: thread.id.clone(),
            num_turns: 1,
        })
        .await?;
    let rollback_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(rollback_id)),
    )
    .await??;
    let rollback: ThreadRollbackResponse = to_response::<ThreadRollbackResponse>(rollback_resp)?;

    assert_eq!(rollback.thread.id, thread.id);
    assert_eq!(rollback.thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(rollback.thread.status, ThreadStatus::Idle);
    assert!(rollback.thread.resident);
    assert_eq!(rollback.thread.name.as_deref(), Some(thread_name));

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
    let read: ThreadReadResponse = to_response::<ThreadReadResponse>(read_resp)?;
    assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(read.thread.status, ThreadStatus::Idle);
    assert!(read.thread.resident);
    assert_eq!(read.thread.name.as_deref(), Some(thread_name));

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
        .expect("thread/loaded/read should include rolled back named resident thread");
    assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(loaded_thread.status, ThreadStatus::Idle);
    assert!(loaded_thread.resident);
    assert_eq!(loaded_thread.name.as_deref(), Some(thread_name));

    let list_id = mcp
        .send_thread_list_request(ThreadListParams {
            cursor: None,
            limit: Some(20),
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
    let listed_thread = data
        .iter()
        .find(|listed_thread| listed_thread.id == thread.id)
        .expect("thread/list should include rolled back named resident thread");
    assert_eq!(listed_thread.mode, ThreadMode::ResidentAssistant);
    assert_eq!(listed_thread.status, ThreadStatus::Idle);
    assert!(listed_thread.resident);
    assert_eq!(listed_thread.name.as_deref(), Some(thread_name));

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
    let resumed: ThreadResumeResponse = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resumed.thread.mode, ThreadMode::ResidentAssistant);
    assert!(resumed.thread.resident);
    assert_eq!(resumed.thread.name.as_deref(), Some(thread_name));

    Ok(())
}

fn create_config_toml(codex_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
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
