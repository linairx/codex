use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_fake_rollout;
use app_test_support::rollout_path;
use app_test_support::to_response;
use chrono::Utc;
use codex_app_server_protocol::ConversationSummary;
use codex_app_server_protocol::GetConversationSummaryParams;
use codex_app_server_protocol::GetConversationSummaryResponse;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const FILENAME_TS: &str = "2025-01-02T12-00-00";
const META_RFC3339: &str = "2025-01-02T12:00:00Z";
const PREVIEW: &str = "Summarize this conversation";
const MODEL_PROVIDER: &str = "openai";

fn expected_summary(conversation_id: ThreadId, path: PathBuf) -> ConversationSummary {
    ConversationSummary {
        conversation_id,
        path,
        preview: PREVIEW.to_string(),
        timestamp: Some(META_RFC3339.to_string()),
        updated_at: Some(META_RFC3339.to_string()),
        model_provider: MODEL_PROVIDER.to_string(),
        cwd: PathBuf::from("/"),
        cli_version: "0.0.0".to_string(),
        source: SessionSource::Cli,
        git_info: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_thread_id_reads_rollout() -> Result<()> {
    let codex_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        Some(MODEL_PROVIDER),
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let expected = expected_summary(
        thread_id,
        std::fs::canonicalize(rollout_path(
            codex_home.path(),
            FILENAME_TS,
            &conversation_id,
        ))?,
    );

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::ThreadId {
            conversation_id: thread_id,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(received.summary, expected);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_relative_rollout_path_resolves_from_codex_home() -> Result<()>
{
    let codex_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        Some(MODEL_PROVIDER),
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let rollout_path = rollout_path(codex_home.path(), FILENAME_TS, &conversation_id);
    let relative_path = rollout_path.strip_prefix(codex_home.path())?.to_path_buf();
    let expected = expected_summary(thread_id, std::fs::canonicalize(rollout_path)?);

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::RolloutPath {
            rollout_path: relative_path,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(received.summary, expected);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_thread_id_repairs_missing_summary_for_existing_sqlite_row()
-> Result<()> {
    let codex_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        codex_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        Some(MODEL_PROVIDER),
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let rollout_path = rollout_path(codex_home.path(), FILENAME_TS, &conversation_id);

    let state_runtime = StateRuntime::init(codex_home.path().to_path_buf(), MODEL_PROVIDER.into())
        .await
        .map_err(std::io::Error::other)?;
    state_runtime
        .mark_backfill_complete(/*last_watermark*/ None)
        .await
        .map_err(std::io::Error::other)?;

    let created_at = Utc::now();
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path.clone(),
        created_at,
        SessionSource::Cli,
    );
    builder.cwd = PathBuf::from("/");
    builder.model_provider = Some(MODEL_PROVIDER.to_string());
    builder.cli_version = Some("0.0.0".to_string());
    let inserted = state_runtime
        .insert_thread_if_absent(&builder.build(MODEL_PROVIDER))
        .await
        .map_err(std::io::Error::other)?;
    assert!(
        inserted,
        "test setup should insert an incomplete sqlite row"
    );

    let expected = expected_summary(thread_id, std::fs::canonicalize(&rollout_path)?);

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::ThreadId {
            conversation_id: thread_id,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(received.summary, expected);

    let persisted = state_runtime
        .get_thread(thread_id)
        .await
        .map_err(std::io::Error::other)?
        .expect("thread metadata should exist after repair");
    assert_eq!(persisted.first_user_message.as_deref(), Some(PREVIEW));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_thread_id_uses_loaded_external_rollout_path() -> Result<()> {
    const EXTERNAL_MODEL_PROVIDER: &str = "mock_provider";

    let codex_home = TempDir::new()?;
    let config_toml = codex_home.path().join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "{EXTERNAL_MODEL_PROVIDER}"

[model_providers.{EXTERNAL_MODEL_PROVIDER}]
name = "Mock provider for test"
base_url = "http://127.0.0.1:9/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )?;

    let external_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        external_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        Some(EXTERNAL_MODEL_PROVIDER),
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let external_rollout = std::fs::canonicalize(rollout_path(
        external_home.path(),
        FILENAME_TS,
        &conversation_id,
    ))?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_request_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id,
            path: Some(external_rollout.clone()),
            ..Default::default()
        })
        .await?;
    let resume_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_request_id)),
    )
    .await??;
    let _: ThreadResumeResponse = to_response(resume_response)?;

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::ThreadId {
            conversation_id: thread_id,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(
        received.summary,
        ConversationSummary {
            conversation_id: thread_id,
            path: external_rollout,
            preview: PREVIEW.to_string(),
            timestamp: Some(META_RFC3339.to_string()),
            updated_at: Some(META_RFC3339.to_string()),
            model_provider: EXTERNAL_MODEL_PROVIDER.to_string(),
            cwd: PathBuf::from("/"),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::Cli,
            git_info: None,
        }
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_thread_id_uses_loaded_provider_override_for_external_rollout()
-> Result<()> {
    const OVERRIDE_PROVIDER: &str = "mock_provider";

    let codex_home = TempDir::new()?;
    create_multi_provider_config_toml(codex_home.path())?;

    let external_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        external_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        /*model_provider*/ None,
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let external_rollout = std::fs::canonicalize(rollout_path(
        external_home.path(),
        FILENAME_TS,
        &conversation_id,
    ))?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_request_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id,
            path: Some(external_rollout.clone()),
            model_provider: Some(OVERRIDE_PROVIDER.to_string()),
            ..Default::default()
        })
        .await?;
    let resume_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_request_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response(resume_response)?;
    assert_eq!(thread.model_provider, OVERRIDE_PROVIDER);

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::ThreadId {
            conversation_id: thread_id,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(
        received.summary,
        ConversationSummary {
            conversation_id: thread_id,
            path: external_rollout,
            preview: PREVIEW.to_string(),
            timestamp: Some(META_RFC3339.to_string()),
            updated_at: Some(META_RFC3339.to_string()),
            model_provider: OVERRIDE_PROVIDER.to_string(),
            cwd: PathBuf::from("/"),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::Cli,
            git_info: None,
        }
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_conversation_summary_by_thread_id_repairs_missing_summary_with_loaded_provider_override()
-> Result<()> {
    const OVERRIDE_PROVIDER: &str = "mock_provider";

    let codex_home = TempDir::new()?;
    create_multi_provider_config_toml(codex_home.path())?;

    let external_home = TempDir::new()?;
    let conversation_id = create_fake_rollout(
        external_home.path(),
        FILENAME_TS,
        META_RFC3339,
        PREVIEW,
        /*model_provider*/ None,
        /*git_info*/ None,
    )?;
    let thread_id = ThreadId::from_string(&conversation_id)?;
    let external_rollout = std::fs::canonicalize(rollout_path(
        external_home.path(),
        FILENAME_TS,
        &conversation_id,
    ))?;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_request_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id,
            path: Some(external_rollout.clone()),
            model_provider: Some(OVERRIDE_PROVIDER.to_string()),
            ..Default::default()
        })
        .await?;
    let resume_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_request_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response(resume_response)?;
    assert_eq!(thread.model_provider, OVERRIDE_PROVIDER);

    let state_runtime =
        StateRuntime::init(codex_home.path().to_path_buf(), "target_provider".into()).await?;
    let mut metadata = state_runtime
        .get_thread(thread_id)
        .await?
        .expect("resume should persist thread metadata");
    metadata.first_user_message = None;
    state_runtime.upsert_thread(&metadata).await?;

    let request_id = mcp
        .send_get_conversation_summary_request(GetConversationSummaryParams::ThreadId {
            conversation_id: thread_id,
        })
        .await?;
    let response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let received: GetConversationSummaryResponse = to_response(response)?;

    assert_eq!(
        received.summary,
        ConversationSummary {
            conversation_id: thread_id,
            path: external_rollout.clone(),
            preview: PREVIEW.to_string(),
            timestamp: Some(META_RFC3339.to_string()),
            updated_at: Some(META_RFC3339.to_string()),
            model_provider: OVERRIDE_PROVIDER.to_string(),
            cwd: PathBuf::from("/"),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::Cli,
            git_info: None,
        }
    );

    let persisted = state_runtime
        .get_thread(thread_id)
        .await?
        .expect("thread metadata should exist after repair");
    assert_eq!(persisted.first_user_message.as_deref(), Some(PREVIEW));
    assert_eq!(persisted.model_provider, OVERRIDE_PROVIDER);

    Ok(())
}

fn create_multi_provider_config_toml(codex_home: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "target_provider"

[model_providers.target_provider]
name = "Target fallback provider for test"
base_url = "http://127.0.0.1:9/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[model_providers.mock_provider]
name = "Mock override provider for test"
base_url = "http://127.0.0.1:9/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
    )
}
