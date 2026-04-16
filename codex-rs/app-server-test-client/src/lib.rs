use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::ArgAction;
use clap::Parser;
use clap::Subcommand;
use codex_app_server_protocol::AccountLoginCompletedNotification;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::CommandExecutionStatus;
use codex_app_server_protocol::DynamicToolSpec;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalParams;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::PermissionGrantScope;
use codex_app_server_protocol::PermissionsRequestApprovalParams;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::ReadOnlyAccess;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::Thread as AppServerThread;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadCloseParams;
use codex_app_server_protocol::ThreadCloseResponse;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadDecrementElicitationParams;
use codex_app_server_protocol::ThreadDecrementElicitationResponse;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadIncrementElicitationParams;
use codex_app_server_protocol::ThreadIncrementElicitationResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMetadataGitInfoUpdateParams;
use codex_app_server_protocol::ThreadMetadataUpdateParams;
use codex_app_server_protocol::ThreadMetadataUpdateResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadRollbackParams;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadUnarchiveParams;
use codex_app_server_protocol::ThreadUnarchiveResponse;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputParams;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput as V2UserInput;
use codex_app_server_protocol::all_thread_source_kinds;
use codex_core::config::Config;
use codex_otel::OtelProvider;
use codex_otel::current_span_w3c_trace_context;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::W3cTraceContext;
use codex_utils_cli::CliConfigOverrides;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::info_span;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::connect;
use tungstenite::stream::MaybeTlsStream;
use url::Url;
use uuid::Uuid;

const NOTIFICATIONS_TO_OPT_OUT: &[&str] = &[
    // v2 item deltas.
    "command/exec/outputDelta",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/fileChange/outputDelta",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
];
const APP_SERVER_GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const APP_SERVER_GRACEFUL_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DEFAULT_ANALYTICS_ENABLED: bool = true;
const OTEL_SERVICE_NAME: &str = "codex-app-server-test-client";
const TRACE_DISABLED_MESSAGE: &str =
    "Not enabled - enable tracing in $CODEX_HOME/config.toml to get a trace URL!";

/// Minimal launcher that initializes the Codex app-server and logs the handshake.
#[derive(Parser)]
#[command(author = "Codex", version, about = "Bootstrap Codex app-server", long_about = None)]
struct Cli {
    /// Path to the `codex` CLI binary. When set, requests use stdio by
    /// spawning `codex app-server` as a child process.
    #[arg(long, env = "CODEX_BIN", global = true)]
    codex_bin: Option<PathBuf>,

    /// Existing websocket server URL to connect to.
    ///
    /// If neither `--codex-bin` nor `--url` is provided, defaults to
    /// `ws://127.0.0.1:4222`.
    #[arg(long, env = "CODEX_APP_SERVER_URL", global = true)]
    url: Option<String>,

    /// Forwarded to the `codex` CLI as `--config key=value`. Repeatable.
    ///
    /// Example:
    ///   `--config 'model_providers.mock.base_url="http://localhost:4010/v2"'`
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append,
        global = true
    )]
    config_overrides: Vec<String>,

    /// JSON array of dynamic tool specs or a single tool object.
    /// Prefix a filename with '@' to read from a file.
    ///
    /// Example:
    ///   --dynamic-tools '[{"name":"demo","description":"Demo","inputSchema":{"type":"object"}}]'
    ///   --dynamic-tools @/path/to/tools.json
    #[arg(long, value_name = "json-or-@file", global = true)]
    dynamic_tools: Option<String>,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Start `codex app-server` on a websocket endpoint in the background.
    ///
    /// Logs are written to:
    ///   `/tmp/codex-app-server-test-client/`
    Serve {
        /// WebSocket listen URL passed to `codex app-server --listen`.
        #[arg(long, default_value = "ws://127.0.0.1:4222")]
        listen: String,
        /// Kill any process listening on the same port before starting.
        #[arg(long, default_value_t = false)]
        kill: bool,
    },
    /// Send a user message through the Codex app-server.
    SendMessage {
        /// User message to send to Codex.
        user_message: String,
    },
    /// Send a user message through the app-server V2 thread/turn APIs.
    SendMessageV2 {
        /// Opt into experimental app-server methods and fields.
        #[arg(long)]
        experimental_api: bool,
        /// User message to send to Codex.
        user_message: String,
    },
    /// Resume or reconnect to a V2 thread by id, then send a user message.
    ResumeMessageV2 {
        /// Existing thread id to resume or reconnect to.
        thread_id: String,
        /// User message to send to Codex.
        user_message: String,
        /// Reconnect using explicit resident-assistant mode instead of legacy resume defaults.
        #[arg(long, default_value_t = false)]
        resident: bool,
    },
    /// Resume or reconnect to a V2 thread and continuously stream notifications/events.
    ///
    /// This command does not auto-exit; stop it with SIGINT/SIGTERM/SIGKILL.
    ThreadResume {
        /// Existing thread id to resume or reconnect to.
        thread_id: String,
        /// Reconnect using explicit resident-assistant mode instead of legacy resume defaults.
        #[arg(long, default_value_t = false)]
        resident: bool,
    },
    /// Initialize the app-server and dump all inbound messages until interrupted.
    ///
    /// This command does not auto-exit; stop it with SIGINT/SIGTERM/SIGKILL.
    Watch,
    /// Initialize the app-server, print thread summaries, then continue streaming summary notifications.
    ///
    /// This command does not auto-exit; stop it with SIGINT/SIGTERM/SIGKILL.
    #[command(name = "watch-summary")]
    WatchSummary {
        /// Number of stored and loaded thread summaries to print during bootstrap.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Start a V2 turn that elicits an ExecCommand approval.
    #[command(name = "trigger-cmd-approval")]
    TriggerCmdApproval {
        /// Optional prompt; defaults to a simple python command.
        user_message: Option<String>,
    },
    /// Start a V2 turn that elicits an ApplyPatch approval.
    #[command(name = "trigger-patch-approval")]
    TriggerPatchApproval {
        /// Optional prompt; defaults to creating a file via apply_patch.
        user_message: Option<String>,
    },
    /// Start a V2 turn that should not elicit an ExecCommand approval.
    #[command(name = "no-trigger-cmd-approval")]
    NoTriggerCmdApproval,
    /// Send two sequential V2 turns in the same thread to test follow-up behavior.
    SendFollowUpV2 {
        /// Initial user message for the first turn.
        first_message: String,
        /// Follow-up user message for the second turn.
        follow_up_message: String,
    },
    /// Trigger zsh-fork multi-subcommand approvals and assert expected approval behavior.
    #[command(name = "trigger-zsh-fork-multi-cmd-approval")]
    TriggerZshForkMultiCmdApproval {
        /// Optional prompt; defaults to an explicit `/usr/bin/true && /usr/bin/true` command.
        user_message: Option<String>,
        /// Minimum number of command-approval callbacks expected in the turn.
        #[arg(long, default_value_t = 2)]
        min_approvals: usize,
        /// One-based approval index to abort (e.g. --abort-on 2 aborts the second approval).
        #[arg(long)]
        abort_on: Option<usize>,
    },
    /// Trigger the ChatGPT login flow and wait for completion.
    TestLogin {
        /// Use the device-code login flow instead of the browser callback flow.
        #[arg(long, default_value_t = false)]
        device_code: bool,
    },
    /// Fetch the current account rate limits from the Codex app-server.
    GetAccountRateLimits,
    /// List the available models from the Codex app-server.
    #[command(name = "model-list")]
    ModelList,
    /// List stored threads from the Codex app-server across interactive and non-interactive sources.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-list")]
    ThreadList {
        /// Opaque pagination cursor returned by a previous `thread-list` call.
        #[arg(long)]
        cursor: Option<String>,
        /// Number of threads to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Read a stored thread summary by id.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-read")]
    ThreadRead {
        /// Existing thread id to read.
        thread_id: String,
        /// Include stored turns in the response.
        #[arg(long, default_value_t = false)]
        include_turns: bool,
    },
    /// Explicitly close a loaded thread runtime by id.
    #[command(name = "thread-close")]
    ThreadClose {
        /// Existing thread id to close.
        thread_id: String,
    },
    /// Fork a stored thread into a new thread.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-fork")]
    ThreadFork {
        /// Existing thread id to fork from.
        thread_id: String,
        /// Fork into a resident assistant thread instead of the default interactive mode.
        #[arg(long, default_value_t = false)]
        resident: bool,
        /// Keep the fork in memory only.
        #[arg(long, default_value_t = false)]
        ephemeral: bool,
    },
    /// Read loaded thread summaries currently resident in memory across interactive and non-interactive sources.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-loaded-read")]
    ThreadLoadedRead {
        /// Opaque pagination cursor returned by a previous `thread-loaded-read` call.
        #[arg(long)]
        cursor: Option<String>,
        /// Number of loaded threads to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// List loaded thread ids currently resident in memory across interactive and non-interactive sources.
    ///
    /// This is an id-only probe; use `thread-loaded-read` when you need resident
    /// `thread.mode`, status, or reconnect semantics.
    #[command(name = "thread-loaded-list")]
    ThreadLoadedList {
        /// Opaque pagination cursor returned by a previous `thread-loaded-list` call.
        #[arg(long)]
        cursor: Option<String>,
        /// Number of loaded thread ids to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Patch stored git metadata for a thread.
    #[command(name = "thread-metadata-update")]
    ThreadMetadataUpdate {
        /// Existing thread id to update.
        thread_id: String,
        /// Replace the stored git sha.
        #[arg(long)]
        sha: Option<String>,
        /// Replace the stored git branch.
        #[arg(long)]
        branch: Option<String>,
        /// Replace the stored git origin URL.
        #[arg(long)]
        origin_url: Option<String>,
    },
    /// Restore an archived thread into the active session directory.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-unarchive")]
    ThreadUnarchive {
        /// Archived thread id to restore.
        thread_id: String,
    },
    /// Roll back the last N turns of a stored thread.
    ///
    /// Prints a compact per-thread summary including `thread.mode`, `status`, and the
    /// derived `resume`/`reconnect` action label.
    #[command(name = "thread-rollback")]
    ThreadRollback {
        /// Existing thread id to roll back.
        thread_id: String,
        /// Number of most recent turns to remove.
        #[arg(long, default_value_t = 1)]
        num_turns: u32,
    },
    /// Increment the out-of-band elicitation pause counter for a thread.
    #[command(name = "thread-increment-elicitation")]
    ThreadIncrementElicitation {
        /// Existing thread id to update.
        thread_id: String,
    },
    /// Decrement the out-of-band elicitation pause counter for a thread.
    #[command(name = "thread-decrement-elicitation")]
    ThreadDecrementElicitation {
        /// Existing thread id to update.
        thread_id: String,
    },
    /// Run the live websocket harness that proves elicitation pause prevents a
    /// 10s unified exec timeout from killing a 15s helper script.
    #[command(name = "live-elicitation-timeout-pause")]
    LiveElicitationTimeoutPause {
        /// Model passed to `thread/start`.
        #[arg(long, env = "CODEX_E2E_MODEL", default_value = "gpt-5")]
        model: String,
        /// Existing workspace path used as the turn cwd.
        #[arg(long, value_name = "path", default_value = ".")]
        workspace: PathBuf,
        /// Helper script to run from the model; defaults to the repo-local
        /// live elicitation hold script.
        #[arg(long, value_name = "path")]
        script: Option<PathBuf>,
        /// Seconds the helper script should sleep while the timeout is paused.
        #[arg(long, default_value_t = 15)]
        hold_seconds: u64,
    },
}

pub async fn run() -> Result<()> {
    let Cli {
        codex_bin,
        url,
        config_overrides,
        dynamic_tools,
        command,
    } = Cli::parse();

    let dynamic_tools = parse_dynamic_tools_arg(&dynamic_tools)?;

    match command {
        CliCommand::Serve { listen, kill } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "serve")?;
            let codex_bin = codex_bin.unwrap_or_else(|| PathBuf::from("codex"));
            serve(&codex_bin, &config_overrides, &listen, kill)
        }
        CliCommand::SendMessage { user_message } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "send-message")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            send_message(&endpoint, &config_overrides, user_message).await
        }
        CliCommand::SendMessageV2 {
            experimental_api,
            user_message,
        } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            send_message_v2_endpoint(
                &endpoint,
                &config_overrides,
                user_message,
                experimental_api,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::ResumeMessageV2 {
            thread_id,
            user_message,
            resident,
        } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            resume_message_v2(
                &endpoint,
                &config_overrides,
                thread_id,
                user_message,
                resident,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::ThreadResume {
            thread_id,
            resident,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-resume")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_resume_follow(&endpoint, &config_overrides, thread_id, resident).await
        }
        CliCommand::Watch => {
            ensure_dynamic_tools_unused(&dynamic_tools, "watch")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            watch(&endpoint, &config_overrides).await
        }
        CliCommand::WatchSummary { limit } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "watch-summary")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            watch_summary(&endpoint, &config_overrides, limit).await
        }
        CliCommand::TriggerCmdApproval { user_message } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            trigger_cmd_approval(&endpoint, &config_overrides, user_message, &dynamic_tools).await
        }
        CliCommand::TriggerPatchApproval { user_message } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            trigger_patch_approval(&endpoint, &config_overrides, user_message, &dynamic_tools).await
        }
        CliCommand::NoTriggerCmdApproval => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            no_trigger_cmd_approval(&endpoint, &config_overrides, &dynamic_tools).await
        }
        CliCommand::SendFollowUpV2 {
            first_message,
            follow_up_message,
        } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            send_follow_up_v2(
                &endpoint,
                &config_overrides,
                first_message,
                follow_up_message,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::TriggerZshForkMultiCmdApproval {
            user_message,
            min_approvals,
            abort_on,
        } => {
            let endpoint = resolve_endpoint(codex_bin, url)?;
            trigger_zsh_fork_multi_cmd_approval(
                &endpoint,
                &config_overrides,
                user_message,
                min_approvals,
                abort_on,
                &dynamic_tools,
            )
            .await
        }
        CliCommand::TestLogin { device_code } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "test-login")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            test_login(&endpoint, &config_overrides, device_code).await
        }
        CliCommand::GetAccountRateLimits => {
            ensure_dynamic_tools_unused(&dynamic_tools, "get-account-rate-limits")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            get_account_rate_limits(&endpoint, &config_overrides).await
        }
        CliCommand::ModelList => {
            ensure_dynamic_tools_unused(&dynamic_tools, "model-list")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            model_list(&endpoint, &config_overrides).await
        }
        CliCommand::ThreadList { cursor, limit } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-list")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_list(&endpoint, &config_overrides, cursor, limit).await
        }
        CliCommand::ThreadRead {
            thread_id,
            include_turns,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-read")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_read(&endpoint, &config_overrides, thread_id, include_turns).await
        }
        CliCommand::ThreadClose { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-close")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_close(&endpoint, &config_overrides, thread_id).await
        }
        CliCommand::ThreadFork {
            thread_id,
            resident,
            ephemeral,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-fork")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_fork(&endpoint, &config_overrides, thread_id, resident, ephemeral).await
        }
        CliCommand::ThreadLoadedRead { cursor, limit } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-loaded-read")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_loaded_read(&endpoint, &config_overrides, cursor, limit).await
        }
        CliCommand::ThreadLoadedList { cursor, limit } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-loaded-list")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_loaded_list(&endpoint, &config_overrides, cursor, limit).await
        }
        CliCommand::ThreadMetadataUpdate {
            thread_id,
            sha,
            branch,
            origin_url,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-metadata-update")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_metadata_update(
                &endpoint,
                &config_overrides,
                thread_id,
                sha,
                branch,
                origin_url,
            )
            .await
        }
        CliCommand::ThreadUnarchive { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-unarchive")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_unarchive(&endpoint, &config_overrides, thread_id).await
        }
        CliCommand::ThreadRollback {
            thread_id,
            num_turns,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-rollback")?;
            let endpoint = resolve_endpoint(codex_bin, url)?;
            thread_rollback(&endpoint, &config_overrides, thread_id, num_turns).await
        }
        CliCommand::ThreadIncrementElicitation { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-increment-elicitation")?;
            let url = resolve_shared_websocket_url(codex_bin, url, "thread-increment-elicitation")?;
            thread_increment_elicitation(&url, thread_id)
        }
        CliCommand::ThreadDecrementElicitation { thread_id } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "thread-decrement-elicitation")?;
            let url = resolve_shared_websocket_url(codex_bin, url, "thread-decrement-elicitation")?;
            thread_decrement_elicitation(&url, thread_id)
        }
        CliCommand::LiveElicitationTimeoutPause {
            model,
            workspace,
            script,
            hold_seconds,
        } => {
            ensure_dynamic_tools_unused(&dynamic_tools, "live-elicitation-timeout-pause")?;
            live_elicitation_timeout_pause(
                codex_bin,
                url,
                &config_overrides,
                model,
                workspace,
                script,
                hold_seconds,
            )
        }
    }
}

enum Endpoint {
    SpawnCodex(PathBuf),
    ConnectWs(String),
}

struct BackgroundAppServer {
    process: Child,
    url: String,
}

fn resolve_endpoint(codex_bin: Option<PathBuf>, url: Option<String>) -> Result<Endpoint> {
    if codex_bin.is_some() && url.is_some() {
        bail!("--codex-bin and --url are mutually exclusive");
    }
    if let Some(codex_bin) = codex_bin {
        return Ok(Endpoint::SpawnCodex(codex_bin));
    }
    if let Some(url) = url {
        return Ok(Endpoint::ConnectWs(url));
    }
    Ok(Endpoint::ConnectWs("ws://127.0.0.1:4222".to_string()))
}

fn resolve_shared_websocket_url(
    codex_bin: Option<PathBuf>,
    url: Option<String>,
    command: &str,
) -> Result<String> {
    if codex_bin.is_some() {
        bail!(
            "{command} requires --url or an already-running websocket app-server; --codex-bin would spawn a private stdio app-server instead"
        );
    }

    Ok(url.unwrap_or_else(|| "ws://127.0.0.1:4222".to_string()))
}

impl BackgroundAppServer {
    fn spawn(codex_bin: &Path, config_overrides: &[String]) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .context("failed to reserve a local port for websocket app-server")?;
        let addr = listener.local_addr()?;
        drop(listener);

        let url = format!("ws://{addr}");
        let mut cmd = Command::new(codex_bin);
        if let Some(codex_bin_parent) = codex_bin.parent() {
            let mut path = OsString::from(codex_bin_parent.as_os_str());
            if let Some(existing_path) = std::env::var_os("PATH") {
                path.push(":");
                path.push(existing_path);
            }
            cmd.env("PATH", path);
        }
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }
        let process = cmd
            .arg("app-server")
            .arg("--listen")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{}` app-server", codex_bin.display()))?;

        Ok(Self { process, url })
    }
}

impl Drop for BackgroundAppServer {
    fn drop(&mut self) {
        if let Ok(Some(status)) = self.process.try_wait() {
            println!("[background app-server exited: {status}]");
            return;
        }

        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn serve(codex_bin: &Path, config_overrides: &[String], listen: &str, kill: bool) -> Result<()> {
    let runtime_dir = PathBuf::from("/tmp/codex-app-server-test-client");
    fs::create_dir_all(&runtime_dir)
        .with_context(|| format!("failed to create runtime dir {}", runtime_dir.display()))?;
    let log_path = runtime_dir.join("app-server.log");
    if kill {
        kill_listeners_on_same_port(listen)?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;
    let log_file_stderr = log_file
        .try_clone()
        .with_context(|| format!("failed to clone log file handle {}", log_path.display()))?;

    let mut cmdline = format!(
        "tail -f /dev/null | RUST_BACKTRACE=full RUST_LOG=warn,codex_=trace {}",
        shell_quote(&codex_bin.display().to_string())
    );
    for override_kv in config_overrides {
        cmdline.push_str(&format!(" --config {}", shell_quote(override_kv)));
    }
    cmdline.push_str(&format!(" app-server --listen {}", shell_quote(listen)));

    let child = Command::new("nohup")
        .arg("sh")
        .arg("-c")
        .arg(cmdline)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_stderr))
        .spawn()
        .with_context(|| format!("failed to start `{}` app-server", codex_bin.display()))?;

    let pid = child.id();

    println!("started codex app-server");
    println!("listen: {listen}");
    println!("pid: {pid} (launcher process)");
    println!("log: {}", log_path.display());

    Ok(())
}

fn kill_listeners_on_same_port(listen: &str) -> Result<()> {
    let url = Url::parse(listen).with_context(|| format!("invalid --listen URL `{listen}`"))?;
    let port = url
        .port_or_known_default()
        .with_context(|| format!("unable to infer port from --listen URL `{listen}`"))?;

    let output = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-tiTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .output()
        .with_context(|| format!("failed to run lsof for port {port}"))?;

    if !output.status.success() {
        return Ok(());
    }

    let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        return Ok(());
    }

    for pid in pids {
        println!("killing listener pid {pid} on port {port}");
        let pid_str = pid.to_string();
        let term_status = Command::new("kill")
            .arg(&pid_str)
            .status()
            .with_context(|| format!("failed to send SIGTERM to pid {pid}"))?;
        if !term_status.success() {
            continue;
        }
    }

    thread::sleep(Duration::from_millis(300));

    let output = Command::new("lsof")
        .arg("-nP")
        .arg(format!("-tiTCP:{port}"))
        .arg("-sTCP:LISTEN")
        .output()
        .with_context(|| format!("failed to re-check listeners on port {port}"))?;
    if !output.status.success() {
        return Ok(());
    }
    let remaining: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect();
    for pid in remaining {
        println!("force killing remaining listener pid {pid} on port {port}");
        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
    }

    Ok(())
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

struct SendMessagePolicies<'a> {
    command_name: &'static str,
    experimental_api: bool,
    approval_policy: Option<AskForApproval>,
    sandbox_policy: Option<SandboxPolicy>,
    dynamic_tools: &'a Option<Vec<DynamicToolSpec>>,
}

async fn send_message(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
) -> Result<()> {
    let dynamic_tools = None;
    send_message_v2_with_policies(
        endpoint,
        config_overrides,
        user_message,
        SendMessagePolicies {
            command_name: "send-message",
            experimental_api: false,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools: &dynamic_tools,
        },
    )
    .await
}

pub async fn send_message_v2(
    codex_bin: &Path,
    config_overrides: &[String],
    user_message: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let endpoint = Endpoint::SpawnCodex(codex_bin.to_path_buf());
    send_message_v2_endpoint(
        &endpoint,
        config_overrides,
        user_message,
        /*experimental_api*/ true,
        dynamic_tools,
    )
    .await
}

async fn send_message_v2_endpoint(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
    experimental_api: bool,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    if dynamic_tools.is_some() && !experimental_api {
        bail!("--dynamic-tools requires --experimental-api for send-message-v2");
    }

    send_message_v2_with_policies(
        endpoint,
        config_overrides,
        user_message,
        SendMessagePolicies {
            command_name: "send-message-v2",
            experimental_api,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools,
        },
    )
    .await
}

async fn trigger_zsh_fork_multi_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    min_approvals: usize,
    abort_on: Option<usize>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    if let Some(abort_on) = abort_on
        && abort_on == 0
    {
        bail!("--abort-on must be >= 1 when provided");
    }

    let default_prompt = "Run this exact command using shell command execution without rewriting or splitting it: /usr/bin/true && /usr/bin/true";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());

    with_client(
        "trigger-zsh-fork-multi-cmd-approval",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize()?;
            println!("< initialize response: {initialize:?}");

            let mut thread_params = interactive_thread_start_params();
            thread_params.dynamic_tools = dynamic_tools.clone();
            let thread_response = client.thread_start(thread_params)?;
            println!("< thread/start response: {thread_response:?}");
            print_thread_response_summary("thread/start", &thread_response.thread);

            client.command_approval_behavior = match abort_on {
                Some(index) => CommandApprovalBehavior::AbortOn(index),
                None => CommandApprovalBehavior::AlwaysAccept,
            };
            client.command_approval_count = 0;
            client.command_approval_item_ids.clear();
            client.command_execution_statuses.clear();
            client.last_turn_status = None;

            let mut turn_params = TurnStartParams {
                thread_id: thread_response.thread.id.clone(),
                input: vec![V2UserInput::Text {
                    text: message,
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            };
            turn_params.approval_policy = Some(AskForApproval::OnRequest);
            turn_params.sandbox_policy = Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            });

            let turn_response = client.turn_start(turn_params)?;
            println!("< turn/start response: {turn_response:?}");
            client.stream_turn(&thread_response.thread.id, &turn_response.turn.id)?;

            if client.command_approval_count < min_approvals {
                bail!(
                    "expected at least {min_approvals} command approvals, got {}",
                    client.command_approval_count
                );
            }
            let mut approvals_per_item = std::collections::BTreeMap::new();
            for item_id in &client.command_approval_item_ids {
                *approvals_per_item.entry(item_id.clone()).or_insert(0usize) += 1;
            }
            let max_approvals_for_one_item =
                approvals_per_item.values().copied().max().unwrap_or(0);
            if max_approvals_for_one_item < min_approvals {
                bail!(
                    "expected at least {min_approvals} approvals for one command item, got max {max_approvals_for_one_item} with map {approvals_per_item:?}"
                );
            }

            let last_command_status = client.command_execution_statuses.last();
            if abort_on.is_none() {
                if last_command_status != Some(&CommandExecutionStatus::Completed) {
                    bail!("expected completed command execution, got {last_command_status:?}");
                }
                if client.last_turn_status != Some(TurnStatus::Completed) {
                    bail!(
                        "expected completed turn in all-accept flow, got {:?}",
                        client.last_turn_status
                    );
                }
            } else if last_command_status == Some(&CommandExecutionStatus::Completed) {
                bail!(
                    "expected non-completed command execution in mixed approval/decline flow, got {last_command_status:?}"
                );
            }

            println!(
                "[zsh-fork multi-approval summary] approvals={}, approvals_per_item={approvals_per_item:?}, command_statuses={:?}, turn_status={:?}",
                client.command_approval_count,
                client.command_execution_statuses,
                client.last_turn_status
            );

            Ok(())
        },
    )
    .await
}

fn interactive_thread_start_params() -> ThreadStartParams {
    ThreadStartParams {
        mode: Some(ThreadMode::Interactive),
        ..Default::default()
    }
}

async fn resume_message_v2(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    user_message: String,
    resident: bool,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    ensure_dynamic_tools_unused(dynamic_tools, "resume-message-v2")?;

    with_client("resume-message-v2", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let resume_response =
            client.thread_resume(explicit_thread_resume_params(thread_id, resident))?;
        println!("< thread/resume response: {resume_response:?}");
        print_thread_response_summary("thread/resume", &resume_response.thread);

        let turn_response = client.turn_start(TurnStartParams {
            thread_id: resume_response.thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: user_message,
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })?;
        println!("< turn/start response: {turn_response:?}");

        client.stream_turn(&resume_response.thread.id, &turn_response.turn.id)?;

        Ok(())
    })
    .await
}

async fn thread_resume_follow(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    resident: bool,
) -> Result<()> {
    with_client("thread-resume", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let resume_response =
            client.thread_resume(explicit_thread_resume_params(thread_id, resident))?;
        println!("< thread/resume response: {resume_response:?}");
        print_thread_response_summary("thread/resume", &resume_response.thread);
        println!("< streaming notifications until process is terminated");

        client.stream_notifications_forever()
    })
    .await
}

fn explicit_thread_resume_params(thread_id: String, resident: bool) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id,
        mode: if resident {
            Some(ThreadMode::ResidentAssistant)
        } else {
            None
        },
        ..Default::default()
    }
}

async fn watch(endpoint: &Endpoint, config_overrides: &[String]) -> Result<()> {
    with_client("watch", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");
        println!("< streaming inbound messages until process is terminated");

        client.stream_notifications_forever()
    })
    .await
}

async fn watch_summary(endpoint: &Endpoint, config_overrides: &[String], limit: u32) -> Result<()> {
    with_client("watch-summary", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let thread_list = client.thread_list(ThreadListParams {
            cursor: None,
            limit: Some(limit),
            sort_key: None,
            model_providers: None,
            source_kinds: Some(all_thread_source_kinds()),
            archived: None,
            cwd: None,
            search_term: None,
        })?;
        print_thread_list_summary(&thread_list);

        let loaded_read = client.thread_loaded_read(ThreadLoadedReadParams {
            cursor: None,
            limit: Some(limit),
            model_providers: None,
            source_kinds: Some(all_thread_source_kinds()),
            cwd: None,
        })?;
        print_thread_collection_summary_with_cursor(
            "thread/loaded/read",
            &loaded_read.data,
            loaded_read.next_cursor.as_deref(),
        );

        println!("< streaming summary notifications until process is terminated");
        client.stream_notifications_forever()
    })
    .await
}

async fn trigger_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let default_prompt =
        "Run `touch /tmp/should-trigger-approval` so I can confirm the file exists.";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());
    send_message_v2_with_policies(
        endpoint,
        config_overrides,
        message,
        SendMessagePolicies {
            command_name: "trigger-cmd-approval",
            experimental_api: true,
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox_policy: Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            }),
            dynamic_tools,
        },
    )
    .await
}

async fn trigger_patch_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: Option<String>,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let default_prompt =
        "Create a file named APPROVAL_DEMO.txt containing a short hello message using apply_patch.";
    let message = user_message.unwrap_or_else(|| default_prompt.to_string());
    send_message_v2_with_policies(
        endpoint,
        config_overrides,
        message,
        SendMessagePolicies {
            command_name: "trigger-patch-approval",
            experimental_api: true,
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox_policy: Some(SandboxPolicy::ReadOnly {
                access: ReadOnlyAccess::FullAccess,
                network_access: false,
            }),
            dynamic_tools,
        },
    )
    .await
}

async fn no_trigger_cmd_approval(
    endpoint: &Endpoint,
    config_overrides: &[String],
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    let prompt = "Run `touch should_not_trigger_approval.txt`";
    send_message_v2_with_policies(
        endpoint,
        config_overrides,
        prompt.to_string(),
        SendMessagePolicies {
            command_name: "no-trigger-cmd-approval",
            experimental_api: true,
            approval_policy: None,
            sandbox_policy: None,
            dynamic_tools,
        },
    )
    .await
}

async fn send_message_v2_with_policies(
    endpoint: &Endpoint,
    config_overrides: &[String],
    user_message: String,
    policies: SendMessagePolicies<'_>,
) -> Result<()> {
    with_client(
        policies.command_name,
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize_with_experimental_api(policies.experimental_api)?;
            println!("< initialize response: {initialize:?}");

            let mut thread_params = interactive_thread_start_params();
            thread_params.dynamic_tools = policies.dynamic_tools.clone();
            let thread_response = client.thread_start(thread_params)?;
            println!("< thread/start response: {thread_response:?}");
            print_thread_response_summary("thread/start", &thread_response.thread);
            let mut turn_params = TurnStartParams {
                thread_id: thread_response.thread.id.clone(),
                input: vec![V2UserInput::Text {
                    text: user_message,
                    // Test client sends plain text without UI element ranges.
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            };
            turn_params.approval_policy = policies.approval_policy;
            turn_params.sandbox_policy = policies.sandbox_policy;

            let turn_response = client.turn_start(turn_params)?;
            println!("< turn/start response: {turn_response:?}");

            client.stream_turn(&thread_response.thread.id, &turn_response.turn.id)?;

            Ok(())
        },
    )
    .await
}

async fn send_follow_up_v2(
    endpoint: &Endpoint,
    config_overrides: &[String],
    first_message: String,
    follow_up_message: String,
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
) -> Result<()> {
    with_client("send-follow-up-v2", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let mut thread_params = interactive_thread_start_params();
        thread_params.dynamic_tools = dynamic_tools.clone();
        let thread_response = client.thread_start(thread_params)?;
        println!("< thread/start response: {thread_response:?}");
        print_thread_response_summary("thread/start", &thread_response.thread);

        let first_turn_params = TurnStartParams {
            thread_id: thread_response.thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: first_message,
                // Test client sends plain text without UI element ranges.
                text_elements: Vec::new(),
            }],
            ..Default::default()
        };
        let first_turn_response = client.turn_start(first_turn_params)?;
        println!("< turn/start response (initial): {first_turn_response:?}");
        client.stream_turn(&thread_response.thread.id, &first_turn_response.turn.id)?;

        let follow_up_params = TurnStartParams {
            thread_id: thread_response.thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: follow_up_message,
                // Test client sends plain text without UI element ranges.
                text_elements: Vec::new(),
            }],
            ..Default::default()
        };
        let follow_up_response = client.turn_start(follow_up_params)?;
        println!("< turn/start response (follow-up): {follow_up_response:?}");
        client.stream_turn(&thread_response.thread.id, &follow_up_response.turn.id)?;

        Ok(())
    })
    .await
}

async fn test_login(
    endpoint: &Endpoint,
    config_overrides: &[String],
    device_code: bool,
) -> Result<()> {
    with_client("test-login", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let login_response = if device_code {
            client.login_account_chatgpt_device_code()?
        } else {
            client.login_account_chatgpt()?
        };
        println!("< account/login/start response: {login_response:?}");
        let login_id = match login_response {
            LoginAccountResponse::Chatgpt { login_id, auth_url } => {
                println!("Open the following URL in your browser to continue:\n{auth_url}");
                login_id
            }
            LoginAccountResponse::ChatgptDeviceCode {
                login_id,
                verification_url,
                user_code,
            } => {
                println!(
                    "Open the following URL and enter the code to continue:\n{verification_url}\n\nCode: {user_code}"
                );
                login_id
            }
            _ => bail!("expected chatgpt login response"),
        };

        let completion = client.wait_for_account_login_completion(&login_id)?;
        println!("< account/login/completed notification: {completion:?}");

        if completion.success {
            println!("Login succeeded.");
            Ok(())
        } else {
            bail!(
                "login failed: {}",
                completion
                    .error
                    .as_deref()
                    .unwrap_or("unknown error from account/login/completed")
            );
        }
    })
    .await
}

async fn get_account_rate_limits(endpoint: &Endpoint, config_overrides: &[String]) -> Result<()> {
    with_client(
        "get-account-rate-limits",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize()?;
            println!("< initialize response: {initialize:?}");

            let response = client.get_account_rate_limits()?;
            println!("< account/rateLimits/read response: {response:?}");

            Ok(())
        },
    )
    .await
}

async fn model_list(endpoint: &Endpoint, config_overrides: &[String]) -> Result<()> {
    with_client("model-list", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.model_list(ModelListParams::default())?;
        println!("< model/list response: {response:?}");

        Ok(())
    })
    .await
}

async fn thread_list(
    endpoint: &Endpoint,
    config_overrides: &[String],
    cursor: Option<String>,
    limit: u32,
) -> Result<()> {
    with_client("thread-list", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_list(ThreadListParams {
            cursor,
            limit: Some(limit),
            sort_key: None,
            model_providers: None,
            source_kinds: Some(all_thread_source_kinds()),
            archived: None,
            cwd: None,
            search_term: None,
        })?;
        println!("< thread/list response: {response:?}");
        print_thread_list_summary(&response);

        Ok(())
    })
    .await
}

async fn thread_read(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    include_turns: bool,
) -> Result<()> {
    with_client("thread-read", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_read(ThreadReadParams {
            thread_id,
            include_turns,
        })?;
        println!("< thread/read response: {response:?}");
        print_thread_response_summary("thread/read", &response.thread);

        Ok(())
    })
    .await
}

async fn thread_close(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
) -> Result<()> {
    with_client("thread-close", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_close(ThreadCloseParams { thread_id })?;
        println!("< thread/close response: {response:?}");

        Ok(())
    })
    .await
}

async fn thread_fork(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    resident: bool,
    ephemeral: bool,
) -> Result<()> {
    with_client("thread-fork", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_fork(ThreadForkParams {
            thread_id,
            mode: Some(if resident {
                ThreadMode::ResidentAssistant
            } else {
                ThreadMode::Interactive
            }),
            ephemeral,
            ..Default::default()
        })?;
        println!("< thread/fork response: {response:?}");
        print_thread_response_summary("thread/fork", &response.thread);

        Ok(())
    })
    .await
}

async fn thread_loaded_read(
    endpoint: &Endpoint,
    config_overrides: &[String],
    cursor: Option<String>,
    limit: u32,
) -> Result<()> {
    with_client("thread-loaded-read", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_loaded_read(ThreadLoadedReadParams {
            cursor,
            limit: Some(limit),
            model_providers: None,
            source_kinds: Some(all_thread_source_kinds()),
            cwd: None,
        })?;
        println!("< thread/loaded/read response: {response:?}");
        print_thread_collection_summary_with_cursor(
            "thread/loaded/read",
            &response.data,
            response.next_cursor.as_deref(),
        );

        Ok(())
    })
    .await
}

async fn thread_loaded_list(
    endpoint: &Endpoint,
    config_overrides: &[String],
    cursor: Option<String>,
    limit: u32,
) -> Result<()> {
    with_client("thread-loaded-list", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_loaded_list(ThreadLoadedListParams {
            cursor,
            limit: Some(limit),
            model_providers: None,
            source_kinds: Some(all_thread_source_kinds()),
            cwd: None,
        })?;
        println!("< thread/loaded/list response: {response:?}");
        print_thread_id_collection_summary_with_cursor(
            "thread/loaded/list",
            &response.data,
            response.next_cursor.as_deref(),
        );

        Ok(())
    })
    .await
}

async fn thread_metadata_update(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    sha: Option<String>,
    branch: Option<String>,
    origin_url: Option<String>,
) -> Result<()> {
    if sha.is_none() && branch.is_none() && origin_url.is_none() {
        bail!("provide at least one of --sha, --branch, or --origin-url");
    }

    with_client(
        "thread-metadata-update",
        endpoint,
        config_overrides,
        |client| {
            let initialize = client.initialize()?;
            println!("< initialize response: {initialize:?}");

            let response = client.thread_metadata_update(ThreadMetadataUpdateParams {
                thread_id,
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: sha.map(Some),
                    branch: branch.map(Some),
                    origin_url: origin_url.map(Some),
                }),
            })?;
            println!("< thread/metadata/update response: {response:?}");
            print_thread_response_summary("thread/metadata/update", &response.thread);

            Ok(())
        },
    )
    .await
}

async fn thread_unarchive(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
) -> Result<()> {
    with_client("thread-unarchive", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_unarchive(ThreadUnarchiveParams { thread_id })?;
        println!("< thread/unarchive response: {response:?}");
        print_thread_response_summary("thread/unarchive", &response.thread);

        Ok(())
    })
    .await
}

async fn thread_rollback(
    endpoint: &Endpoint,
    config_overrides: &[String],
    thread_id: String,
    num_turns: u32,
) -> Result<()> {
    with_client("thread-rollback", endpoint, config_overrides, |client| {
        let initialize = client.initialize()?;
        println!("< initialize response: {initialize:?}");

        let response = client.thread_rollback(ThreadRollbackParams {
            thread_id,
            num_turns,
        })?;
        println!("< thread/rollback response: {response:?}");
        print_thread_response_summary("thread/rollback", &response.thread);

        Ok(())
    })
    .await
}

async fn with_client<T>(
    command_name: &'static str,
    endpoint: &Endpoint,
    config_overrides: &[String],
    f: impl FnOnce(&mut CodexClient) -> Result<T>,
) -> Result<T> {
    let tracing = TestClientTracing::initialize(config_overrides).await?;
    let command_span = info_span!(
        "app_server_test_client.command",
        otel.kind = "client",
        otel.name = command_name,
        app_server_test_client.command = command_name,
    );
    let trace_summary = command_span.in_scope(|| TraceSummary::capture(tracing.traces_enabled));
    let result = command_span.in_scope(|| {
        let mut client = CodexClient::connect(endpoint, config_overrides)?;
        f(&mut client)
    });
    print_trace_summary(&trace_summary);
    result
}

fn print_thread_response_summary(label: &str, thread: &AppServerThread) {
    println!("{}", thread_response_summary_line(label, thread));
}

fn print_thread_list_summary(response: &ThreadListResponse) {
    for line in thread_collection_summary_lines_with_cursor(
        "thread/list",
        &response.data,
        response.next_cursor.as_deref(),
    ) {
        println!("{line}");
    }
}

fn print_thread_collection_summary_with_cursor(
    label: &str,
    threads: &[AppServerThread],
    next_cursor: Option<&str>,
) {
    for line in thread_collection_summary_lines_with_cursor(label, threads, next_cursor) {
        println!("{line}");
    }
}

fn print_thread_id_collection_summary_with_cursor(
    label: &str,
    thread_ids: &[String],
    next_cursor: Option<&str>,
) {
    for line in thread_id_collection_summary_lines_with_cursor(label, thread_ids, next_cursor) {
        println!("{line}");
    }
}

fn thread_response_summary_line(label: &str, thread: &AppServerThread) -> String {
    format!("< {label} summary: {}", thread_summary_fields(thread))
}

fn thread_collection_summary_lines_with_cursor(
    label: &str,
    threads: &[AppServerThread],
    next_cursor: Option<&str>,
) -> Vec<String> {
    if threads.is_empty() {
        let mut lines = vec![format!("< {label} summary: no threads")];
        if let Some(next_cursor) = next_cursor {
            lines.push(format!("< {label} next_cursor: {next_cursor}"));
        }
        return lines;
    }

    let mut lines = vec![format!("< {label} summary:")];
    lines.extend(
        threads
            .iter()
            .map(thread_summary_fields)
            .map(|summary| format!("  - {summary}")),
    );
    if let Some(next_cursor) = next_cursor {
        lines.push(format!("< {label} next_cursor: {next_cursor}"));
    }
    lines
}

fn thread_id_collection_summary_lines_with_cursor(
    label: &str,
    thread_ids: &[String],
    next_cursor: Option<&str>,
) -> Vec<String> {
    if thread_ids.is_empty() {
        let mut lines = vec![format!("< {label} summary: no threads")];
        if let Some(next_cursor) = next_cursor {
            lines.push(format!("< {label} next_cursor: {next_cursor}"));
        }
        return lines;
    }

    let mut lines = vec![format!("< {label} summary:")];
    lines.extend(
        thread_ids
            .iter()
            .map(|thread_id| format!("  - id={thread_id}")),
    );
    if let Some(next_cursor) = next_cursor {
        lines.push(format!("< {label} next_cursor: {next_cursor}"));
    }
    lines
}

fn thread_summary_fields(thread: &AppServerThread) -> String {
    format!(
        "id={}, name={}, mode={}, resident={}, status={}, action={}",
        thread.id,
        thread_name_label(thread.name.as_deref()),
        thread_mode_label(thread.mode),
        thread.resident,
        thread_status_label(&thread.status),
        thread_resume_action_label(thread.mode)
    )
}

fn thread_name_label(thread_name: Option<&str>) -> &str {
    thread_name.unwrap_or("-")
}

fn thread_mode_label(mode: ThreadMode) -> &'static str {
    match mode {
        ThreadMode::Interactive => "interactive",
        ThreadMode::ResidentAssistant => "residentAssistant",
    }
}

fn thread_resume_action_label(mode: ThreadMode) -> &'static str {
    match mode {
        ThreadMode::Interactive => "resume",
        ThreadMode::ResidentAssistant => "reconnect",
    }
}

fn thread_status_label(status: &codex_app_server_protocol::ThreadStatus) -> String {
    match status {
        codex_app_server_protocol::ThreadStatus::NotLoaded => "NotLoaded".to_string(),
        codex_app_server_protocol::ThreadStatus::Idle => "Idle".to_string(),
        codex_app_server_protocol::ThreadStatus::SystemError => "SystemError".to_string(),
        codex_app_server_protocol::ThreadStatus::Active { active_flags } => {
            let active_flags = if active_flags.is_empty() {
                "none".to_string()
            } else {
                active_flags
                    .iter()
                    .copied()
                    .map(thread_active_flag_label)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!("Active(flags={active_flags})")
        }
    }
}

fn thread_active_flag_label(active_flag: ThreadActiveFlag) -> &'static str {
    match active_flag {
        ThreadActiveFlag::WaitingOnApproval => "waitingOnApproval",
        ThreadActiveFlag::WaitingOnUserInput => "waitingOnUserInput",
        ThreadActiveFlag::BackgroundTerminalRunning => "backgroundTerminalRunning",
        ThreadActiveFlag::WorkspaceChanged => "workspaceChanged",
    }
}

fn thread_status_changed_summary_line(
    thread_id: &str,
    status: &codex_app_server_protocol::ThreadStatus,
    known_thread: Option<&KnownThreadSummary>,
) -> String {
    match known_thread {
        Some(thread) => format!(
            "id={thread_id}, name={}, mode={}, resident={}, status={}, action={}",
            thread_name_label(thread.name.as_deref()),
            thread_mode_label(thread.mode),
            thread.resident,
            thread_status_label(status),
            thread_resume_action_label(thread.mode)
        ),
        None => format!("id={thread_id}, status={}", thread_status_label(status)),
    }
}

fn thread_name_updated_summary_line(
    thread_id: &str,
    thread_name: Option<&str>,
    known_thread: Option<&KnownThreadSummary>,
) -> String {
    match known_thread {
        Some(thread) => format!(
            "id={thread_id}, name={}, mode={}, resident={}, status={}, action={}",
            thread_name_label(thread_name),
            thread_mode_label(thread.mode),
            thread.resident,
            thread_status_label(&thread.status),
            thread_resume_action_label(thread.mode)
        ),
        None => format!("id={thread_id}, name={}", thread_name_label(thread_name)),
    }
}

fn thread_closed_summary_line(
    thread_id: &str,
    known_thread: Option<&KnownThreadSummary>,
) -> String {
    match known_thread {
        Some(thread) => format!(
            "id={thread_id}, name={}, mode={}, resident={}, status={}, action={}",
            thread_name_label(thread.name.as_deref()),
            thread_mode_label(thread.mode),
            thread.resident,
            thread_status_label(&thread.status),
            thread_resume_action_label(thread.mode)
        ),
        None => format!("id={thread_id}"),
    }
}

fn thread_started_notification_lines(
    thread: &AppServerThread,
    include_raw_debug: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    if include_raw_debug {
        lines.push(format!("< thread/started notification: {thread:?}"));
    }
    lines.push(thread_response_summary_line("thread/started", thread));
    lines
}

fn thread_status_changed_notification_lines(
    thread_id: &str,
    status: &codex_app_server_protocol::ThreadStatus,
    known_thread: Option<&KnownThreadSummary>,
) -> Vec<String> {
    vec![format!(
        "< thread/status/changed summary: {}",
        thread_status_changed_summary_line(thread_id, status, known_thread)
    )]
}

fn thread_name_updated_notification_lines(
    thread_id: &str,
    thread_name: Option<&str>,
    known_thread: Option<&KnownThreadSummary>,
) -> Vec<String> {
    vec![format!(
        "< thread/name/updated summary: {}",
        thread_name_updated_summary_line(thread_id, thread_name, known_thread)
    )]
}

fn thread_closed_notification_lines(
    thread_id: &str,
    known_thread: Option<&KnownThreadSummary>,
) -> Vec<String> {
    vec![format!(
        "< thread/closed summary: {}",
        thread_closed_summary_line(thread_id, known_thread)
    )]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    use super::Cli;
    use super::ClientTransport;
    use super::CodexClient;
    use super::KnownThreadSummary;
    use super::all_thread_source_kinds;
    use super::default_user_input_answers;
    use super::explicit_thread_resume_params;
    use super::granted_permission_profile_from_request;
    use super::interactive_thread_start_params;
    use super::thread_closed_notification_lines;
    use super::thread_closed_summary_line;
    use super::thread_collection_summary_lines_with_cursor;
    use super::thread_id_collection_summary_lines_with_cursor;
    use super::thread_mode_label;
    use super::thread_name_updated_notification_lines;
    use super::thread_name_updated_summary_line;
    use super::thread_response_summary_line;
    use super::thread_resume_action_label;
    use super::thread_started_notification_lines;
    use super::thread_status_changed_notification_lines;
    use super::thread_status_changed_summary_line;
    use super::thread_status_label;
    use super::thread_summary_fields;
    use clap::CommandFactory;
    use codex_app_server_protocol::ClientRequest;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadForkParams;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ThreadStatusChangedNotification;
    use codex_app_server_protocol::ToolRequestUserInputOption;
    use codex_app_server_protocol::ToolRequestUserInputQuestion;
    use codex_app_server_protocol::all_thread_source_kinds as protocol_all_thread_source_kinds;
    use tungstenite::accept;
    use tungstenite::connect;
    use tungstenite::protocol::Message;

    fn make_thread(
        id: &str,
        name: Option<&str>,
        mode: ThreadMode,
        resident: bool,
        status: ThreadStatus,
    ) -> Thread {
        Thread {
            id: id.to_string(),
            forked_from_id: None,
            preview: "preview".to_string(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at: 1,
            updated_at: 2,
            status,
            mode,
            resident,
            path: None,
            cwd: PathBuf::from("/tmp/atlas"),
            cli_version: "1.0.0".to_string(),
            source: SessionSource::Cli,
            agent_nickname: None,
            agent_role: None,
            git_info: None,
            name: name.map(ToString::to_string),
            turns: Vec::new(),
        }
    }

    #[test]
    fn resident_thread_mode_uses_reconnect_label() {
        assert_eq!(
            thread_mode_label(ThreadMode::ResidentAssistant),
            "residentAssistant"
        );
        assert_eq!(
            thread_resume_action_label(ThreadMode::ResidentAssistant),
            "reconnect"
        );
    }

    #[test]
    fn interactive_thread_mode_uses_resume_label() {
        assert_eq!(thread_mode_label(ThreadMode::Interactive), "interactive");
        assert_eq!(
            thread_resume_action_label(ThreadMode::Interactive),
            "resume"
        );
    }

    #[test]
    fn all_thread_source_kinds_cover_interactive_and_non_interactive_sources() {
        assert_eq!(
            all_thread_source_kinds(),
            protocol_all_thread_source_kinds()
        );
    }

    #[test]
    fn help_mentions_resume_or_reconnect_for_thread_commands() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("Resume or reconnect to a V2 thread by id"));
        assert!(help.contains("Resume or reconnect to a V2 thread and continuously stream"));
        assert!(help.contains("watch-summary"));
    }

    #[test]
    fn interactive_thread_start_params_use_explicit_mode() {
        let params = interactive_thread_start_params();

        assert_eq!(params.mode, Some(ThreadMode::Interactive));
        assert!(!params.resident);
    }

    #[test]
    fn thread_fork_request_prefers_mode_over_legacy_resident_flag() {
        let request = ClientRequest::ThreadFork {
            request_id: RequestId::Integer(1),
            params: ThreadForkParams {
                thread_id: "thread-1".to_string(),
                mode: Some(ThreadMode::ResidentAssistant),
                ..Default::default()
            },
        };
        let value = serde_json::to_value(&request).expect("request should serialize");
        let params = value.get("params").expect("request should include params");

        assert_eq!(
            params.get("mode"),
            Some(&serde_json::Value::String("residentAssistant".to_string()))
        );
        assert_eq!(params.get("resident"), None);
    }

    #[test]
    fn explicit_thread_resume_params_use_mode_for_resident_reconnect() {
        let params = explicit_thread_resume_params("thread-1".to_string(), /*resident*/ true);

        assert_eq!(params.mode, Some(ThreadMode::ResidentAssistant));
        assert!(!params.resident);
    }

    #[test]
    fn help_mentions_cursor_for_paginated_thread_commands() {
        let thread_read_help = Cli::command()
            .find_subcommand_mut("thread-read")
            .expect("thread-read subcommand")
            .render_long_help()
            .to_string();
        let thread_fork_help = Cli::command()
            .find_subcommand_mut("thread-fork")
            .expect("thread-fork subcommand")
            .render_long_help()
            .to_string();
        let thread_list_help = Cli::command()
            .find_subcommand_mut("thread-list")
            .expect("thread-list subcommand")
            .render_long_help()
            .to_string();
        let thread_loaded_read_help = Cli::command()
            .find_subcommand_mut("thread-loaded-read")
            .expect("thread-loaded-read subcommand")
            .render_long_help()
            .to_string();
        let thread_loaded_list_help = Cli::command()
            .find_subcommand_mut("thread-loaded-list")
            .expect("thread-loaded-list subcommand")
            .render_long_help()
            .to_string();
        let watch_summary_help = Cli::command()
            .find_subcommand_mut("watch-summary")
            .expect("watch-summary subcommand")
            .render_long_help()
            .to_string();
        let thread_unarchive_help = Cli::command()
            .find_subcommand_mut("thread-unarchive")
            .expect("thread-unarchive subcommand")
            .render_long_help()
            .to_string();
        let thread_rollback_help = Cli::command()
            .find_subcommand_mut("thread-rollback")
            .expect("thread-rollback subcommand")
            .render_long_help()
            .to_string();

        assert!(thread_read_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_read_help.contains("derived"));
        assert!(thread_read_help.contains("`resume`/`reconnect` action label"));
        assert!(thread_fork_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_fork_help.contains("derived"));
        assert!(thread_fork_help.contains("`resume`/`reconnect` action label"));
        assert!(thread_list_help.contains("--cursor <CURSOR>"));
        assert!(thread_list_help.contains(
            "List stored threads from the Codex app-server across interactive and non-interactive sources"
        ));
        assert!(thread_list_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_list_help.contains("derived"));
        assert!(thread_list_help.contains("`resume`/`reconnect` action label"));
        assert!(
            thread_list_help
                .contains("Opaque pagination cursor returned by a previous `thread-list` call")
        );
        assert!(thread_loaded_read_help.contains("--cursor <CURSOR>"));
        assert!(thread_loaded_read_help.contains(
            "Read loaded thread summaries currently resident in memory across interactive and non-interactive"
        ));
        assert!(thread_loaded_read_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_loaded_read_help.contains("derived"));
        assert!(thread_loaded_read_help.contains("`resume`/`reconnect` action label"));
        assert!(
            thread_loaded_read_help.contains(
                "Opaque pagination cursor returned by a previous `thread-loaded-read` call"
            )
        );
        assert!(thread_loaded_list_help.contains("--cursor <CURSOR>"));
        assert!(thread_loaded_list_help.contains(
            "List loaded thread ids currently resident in memory across interactive and non-interactive sources"
        ));
        assert!(
            thread_loaded_list_help.contains(
                "This is an id-only probe; use `thread-loaded-read` when you need resident"
            )
        );
        assert!(thread_loaded_list_help.contains("`thread.mode`, status,"));
        assert!(thread_loaded_list_help.contains("reconnect"));
        assert!(
            thread_loaded_list_help.contains(
                "Opaque pagination cursor returned by a previous `thread-loaded-list` call"
            )
        );
        assert!(watch_summary_help.contains(
            "Initialize the app-server, print thread summaries, then continue streaming summary notifications"
        ));
        assert!(watch_summary_help.contains("--limit <LIMIT>"));
        assert!(
            watch_summary_help
                .contains("Number of stored and loaded thread summaries to print during bootstrap")
        );
        assert!(thread_unarchive_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_unarchive_help.contains("derived"));
        assert!(thread_unarchive_help.contains("`resume`/`reconnect` action label"));
        assert!(thread_rollback_help.contains(
            "Prints a compact per-thread summary including `thread.mode`, `status`, and the"
        ));
        assert!(thread_rollback_help.contains("derived"));
        assert!(thread_rollback_help.contains("`resume`/`reconnect` action label"));
    }

    #[test]
    fn thread_response_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/read", &resident_thread),
            "< thread/read summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_start_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/start", &resident_thread),
            "< thread/start summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_resume_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/resume", &resident_thread),
            "< thread/resume summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_started_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/started", &resident_thread),
            "< thread/started summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_fork_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/fork", &resident_thread),
            "< thread/fork summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_metadata_update_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/metadata/update", &resident_thread),
            "< thread/metadata/update summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_unarchive_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/unarchive", &resident_thread),
            "< thread/unarchive summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_rollback_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_response_summary_line("thread/rollback", &resident_thread),
            "< thread/rollback summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_collection_summary_lines_cover_mode_resident_status_and_action() {
        let threads = vec![
            make_thread(
                "thread-1",
                Some("atlas"),
                ThreadMode::ResidentAssistant,
                true,
                ThreadStatus::Idle,
            ),
            make_thread(
                "thread-2",
                Some("chat"),
                ThreadMode::Interactive,
                false,
                ThreadStatus::NotLoaded,
            ),
        ];

        assert_eq!(
            thread_collection_summary_lines_with_cursor("thread/list", &threads, None),
            vec![
                "< thread/list summary:".to_string(),
                "  - id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect".to_string(),
                "  - id=thread-2, name=chat, mode=interactive, resident=false, status=NotLoaded, action=resume".to_string(),
            ]
        );
    }

    #[test]
    fn thread_loaded_read_summary_lines_cover_mode_resident_status_and_action() {
        let threads = vec![make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        )];

        assert_eq!(
            thread_collection_summary_lines_with_cursor("thread/loaded/read", &threads, None),
            vec![
                "< thread/loaded/read summary:".to_string(),
                "  - id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Active(flags=none), action=reconnect".to_string(),
            ]
        );
    }

    #[test]
    fn empty_thread_collection_summary_lines_use_no_threads_marker() {
        assert_eq!(
            thread_collection_summary_lines_with_cursor("thread/list", &[], None),
            vec!["< thread/list summary: no threads".to_string()]
        );
    }

    #[test]
    fn thread_list_summary_lines_include_next_cursor() {
        let threads = vec![make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        )];

        assert_eq!(
            thread_collection_summary_lines_with_cursor("thread/list", &threads, Some("cursor-2")),
            vec![
                "< thread/list summary:".to_string(),
                "  - id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect".to_string(),
                "< thread/list next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn thread_loaded_read_summary_lines_include_next_cursor() {
        let threads = vec![make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        )];

        assert_eq!(
            thread_collection_summary_lines_with_cursor(
                "thread/loaded/read",
                &threads,
                Some("cursor-2")
            ),
            vec![
                "< thread/loaded/read summary:".to_string(),
                "  - id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Active(flags=none), action=reconnect".to_string(),
                "< thread/loaded/read next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn thread_loaded_list_summary_lines_include_ids_and_next_cursor() {
        assert_eq!(
            thread_id_collection_summary_lines_with_cursor(
                "thread/loaded/list",
                &["thread-1".to_string(), "thread-2".to_string()],
                Some("cursor-2")
            ),
            vec![
                "< thread/loaded/list summary:".to_string(),
                "  - id=thread-1".to_string(),
                "  - id=thread-2".to_string(),
                "< thread/loaded/list next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn thread_loaded_list_summary_lines_do_not_include_mode_or_action_labels() {
        let lines = thread_id_collection_summary_lines_with_cursor(
            "thread/loaded/list",
            &["thread-1".to_string()],
            Some("cursor-2"),
        );
        let rendered = lines.join("\n");

        assert!(!rendered.contains("residentAssistant"));
        assert!(!rendered.contains("interactive"));
        assert!(!rendered.contains("reconnect"));
        assert!(!rendered.contains("action="));
        assert!(!rendered.contains("mode="));
    }

    #[test]
    fn empty_thread_list_summary_lines_can_still_include_next_cursor() {
        assert_eq!(
            thread_collection_summary_lines_with_cursor("thread/list", &[], Some("cursor-2")),
            vec![
                "< thread/list summary: no threads".to_string(),
                "< thread/list next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn empty_thread_loaded_read_summary_lines_can_still_include_next_cursor() {
        assert_eq!(
            thread_collection_summary_lines_with_cursor(
                "thread/loaded/read",
                &[],
                Some("cursor-2")
            ),
            vec![
                "< thread/loaded/read summary: no threads".to_string(),
                "< thread/loaded/read next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn empty_thread_loaded_list_summary_lines_can_still_include_next_cursor() {
        assert_eq!(
            thread_id_collection_summary_lines_with_cursor(
                "thread/loaded/list",
                &[],
                Some("cursor-2")
            ),
            vec![
                "< thread/loaded/list summary: no threads".to_string(),
                "< thread/loaded/list next_cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn thread_summary_fields_mentions_interactive_resume_action() {
        let interactive_thread = make_thread(
            "thread-2",
            Some("chat"),
            ThreadMode::Interactive,
            false,
            ThreadStatus::NotLoaded,
        );

        assert_eq!(
            thread_summary_fields(&interactive_thread),
            "id=thread-2, name=chat, mode=interactive, resident=false, status=NotLoaded, action=resume"
        );
    }

    #[test]
    fn thread_status_changed_summary_line_uses_known_mode_and_action_when_available() {
        let known_thread = KnownThreadSummary {
            name: Some("atlas".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::Idle,
        };

        assert_eq!(
            thread_status_changed_summary_line(
                "thread-1",
                &ThreadStatus::SystemError,
                Some(&known_thread),
            ),
            "id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=SystemError, action=reconnect"
        );
    }

    #[test]
    fn thread_status_changed_refresh_summary_line_mentions_resident_reconnect_action() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::SystemError,
        );

        assert_eq!(
            thread_response_summary_line("thread/status/changed refresh", &resident_thread),
            "< thread/status/changed refresh summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=SystemError, action=reconnect"
        );
    }

    #[test]
    fn thread_status_changed_summary_line_stays_status_only_for_unknown_threads() {
        assert_eq!(
            thread_status_changed_summary_line("thread-unknown", &ThreadStatus::SystemError, None),
            "id=thread-unknown, status=SystemError"
        );
    }

    #[test]
    fn thread_status_label_expands_active_flags_into_stable_summary_text() {
        assert_eq!(
            thread_status_label(&ThreadStatus::Active {
                active_flags: vec![
                    ThreadActiveFlag::WaitingOnApproval,
                    ThreadActiveFlag::WorkspaceChanged,
                ],
            }),
            "Active(flags=waitingOnApproval,workspaceChanged)"
        );
        assert_eq!(
            thread_status_label(&ThreadStatus::Active {
                active_flags: Vec::new(),
            }),
            "Active(flags=none)"
        );
    }

    #[test]
    fn thread_started_notification_lines_include_raw_debug_when_requested() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        let lines =
            thread_started_notification_lines(&resident_thread, /*include_raw_debug*/ true);

        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("< thread/started notification: Thread {"));
        assert_eq!(
            lines[1],
            "< thread/started summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_started_notification_lines_can_emit_summary_only() {
        let resident_thread = make_thread(
            "thread-1",
            Some("atlas"),
            ThreadMode::ResidentAssistant,
            true,
            ThreadStatus::Idle,
        );

        assert_eq!(
            thread_started_notification_lines(&resident_thread, /*include_raw_debug*/ false),
            vec![String::from(
                "< thread/started summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
            )]
        );
    }

    #[test]
    fn thread_status_changed_notification_lines_wrap_summary_line() {
        let known_thread = KnownThreadSummary {
            name: Some("atlas".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::Idle,
        };

        assert_eq!(
            thread_status_changed_notification_lines(
                "thread-1",
                &ThreadStatus::SystemError,
                Some(&known_thread),
            ),
            vec![String::from(
                "< thread/status/changed summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=SystemError, action=reconnect"
            )]
        );
    }

    #[test]
    fn thread_name_updated_summary_line_uses_known_thread_context_when_available() {
        let known_thread = KnownThreadSummary {
            name: Some("old-name".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::Idle,
        };

        assert_eq!(
            thread_name_updated_summary_line("thread-1", Some("atlas"), Some(&known_thread)),
            "id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
        );
    }

    #[test]
    fn thread_name_updated_summary_line_stays_identity_only_for_unknown_threads() {
        assert_eq!(
            thread_name_updated_summary_line("thread-unknown", Some("atlas"), None),
            "id=thread-unknown, name=atlas"
        );
    }

    #[test]
    fn thread_closed_summary_line_uses_known_thread_context_when_available() {
        let known_thread = KnownThreadSummary {
            name: Some("atlas".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::NotLoaded,
        };

        assert_eq!(
            thread_closed_summary_line("thread-1", Some(&known_thread)),
            "id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=NotLoaded, action=reconnect"
        );
    }

    #[test]
    fn thread_closed_summary_line_stays_identity_only_for_unknown_threads() {
        assert_eq!(
            thread_closed_summary_line("thread-unknown", None),
            "id=thread-unknown"
        );
    }

    #[test]
    fn thread_name_updated_notification_lines_wrap_summary_line() {
        let known_thread = KnownThreadSummary {
            name: Some("old-name".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::Idle,
        };

        assert_eq!(
            thread_name_updated_notification_lines("thread-1", Some("atlas"), Some(&known_thread)),
            vec![String::from(
                "< thread/name/updated summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=Idle, action=reconnect"
            )]
        );
    }

    #[test]
    fn thread_closed_notification_lines_wrap_summary_line() {
        let known_thread = KnownThreadSummary {
            name: Some("atlas".to_string()),
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            status: ThreadStatus::NotLoaded,
        };

        assert_eq!(
            thread_closed_notification_lines("thread-1", Some(&known_thread)),
            vec![String::from(
                "< thread/closed summary: id=thread-1, name=atlas, mode=residentAssistant, resident=true, status=NotLoaded, action=reconnect"
            )]
        );
    }

    #[test]
    fn granted_permission_profile_from_request_keeps_requested_permissions() {
        assert_eq!(
            granted_permission_profile_from_request(
                codex_app_server_protocol::RequestPermissionProfile {
                    network: Some(codex_app_server_protocol::AdditionalNetworkPermissions {
                        enabled: Some(true),
                    }),
                    file_system: Some(codex_app_server_protocol::AdditionalFileSystemPermissions {
                        read: Some(vec![]),
                        write: Some(vec![]),
                    }),
                }
            ),
            codex_app_server_protocol::GrantedPermissionProfile {
                network: Some(codex_app_server_protocol::AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(codex_app_server_protocol::AdditionalFileSystemPermissions {
                    read: Some(vec![]),
                    write: Some(vec![]),
                }),
            }
        );
    }

    #[test]
    fn default_user_input_answers_prefers_first_option_and_empty_fallback() {
        assert_eq!(
            default_user_input_answers(&[
                ToolRequestUserInputQuestion {
                    id: "q1".to_string(),
                    header: "Mode".to_string(),
                    question: "Pick one".to_string(),
                    is_other: false,
                    is_secret: false,
                    options: Some(vec![ToolRequestUserInputOption {
                        label: "fast".to_string(),
                        description: "short".to_string(),
                    }]),
                },
                ToolRequestUserInputQuestion {
                    id: "q2".to_string(),
                    header: "Freeform".to_string(),
                    question: "Type".to_string(),
                    is_other: true,
                    is_secret: false,
                    options: None,
                },
            ]),
            HashMap::from([
                (
                    "q1".to_string(),
                    codex_app_server_protocol::ToolRequestUserInputAnswer {
                        answers: vec!["fast".to_string()],
                    },
                ),
                (
                    "q2".to_string(),
                    codex_app_server_protocol::ToolRequestUserInputAnswer {
                        answers: vec![String::new()],
                    },
                ),
            ])
        );
    }

    #[test]
    fn unknown_thread_status_change_refresh_round_trip_restores_known_thread_summary() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should resolve");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("server should accept");
            let mut websocket = accept(stream).expect("websocket handshake should succeed");

            let request_message = websocket.read().expect("thread/read request should arrive");
            let Message::Text(payload) = request_message else {
                panic!("expected text websocket frame");
            };
            let JSONRPCMessage::Request(JSONRPCRequest {
                id, method, params, ..
            }) = serde_json::from_str(&payload).expect("request should decode")
            else {
                panic!("expected JSON-RPC request");
            };
            assert_eq!(method, "thread/read");
            assert_eq!(
                params,
                Some(serde_json::json!({
                    "threadId": "thread-unknown",
                    "includeTurns": false
                }))
            );

            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id,
                result: serde_json::to_value(ThreadReadResponse {
                    thread: make_thread(
                        "thread-unknown",
                        Some("atlas"),
                        ThreadMode::ResidentAssistant,
                        true,
                        ThreadStatus::SystemError,
                    ),
                })
                .expect("thread/read response should serialize"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&response)
                        .expect("response should encode")
                        .into(),
                ))
                .expect("websocket response should send");
        });

        let (socket, _response) =
            connect(format!("ws://{addr}")).expect("websocket client should connect");
        let mut client = CodexClient {
            transport: ClientTransport::WebSocket {
                url: format!("ws://{addr}"),
                socket: Box::new(socket),
            },
            pending_notifications: VecDeque::new(),
            known_threads: Default::default(),
            command_approval_behavior: super::CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        };

        let handled = client.print_stream_notification(
            ServerNotification::ThreadStatusChanged(ThreadStatusChangedNotification {
                thread_id: "thread-unknown".to_string(),
                status: ThreadStatus::SystemError,
            }),
            None,
            None,
        );
        assert!(
            !handled,
            "status notification should not terminate streaming"
        );
        assert_eq!(
            client.known_threads.get("thread-unknown"),
            Some(&KnownThreadSummary {
                name: Some("atlas".to_string()),
                mode: ThreadMode::ResidentAssistant,
                resident: true,
                status: ThreadStatus::SystemError,
            })
        );

        server.join().expect("websocket server should exit cleanly");
    }

    #[test]
    fn unknown_thread_not_loaded_status_change_skips_refresh_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should resolve");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("server should accept");
            let mut websocket = accept(stream).expect("websocket handshake should succeed");
            std::thread::sleep(Duration::from_millis(50));
            websocket
                .close(None)
                .expect("websocket should close cleanly");
        });

        let (socket, _response) =
            connect(format!("ws://{addr}")).expect("websocket client should connect");
        let mut client = CodexClient {
            transport: ClientTransport::WebSocket {
                url: format!("ws://{addr}"),
                socket: Box::new(socket),
            },
            pending_notifications: VecDeque::new(),
            known_threads: Default::default(),
            command_approval_behavior: super::CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        };

        let handled = client.print_stream_notification(
            ServerNotification::ThreadStatusChanged(ThreadStatusChangedNotification {
                thread_id: "thread-unknown".to_string(),
                status: ThreadStatus::NotLoaded,
            }),
            None,
            None,
        );
        assert!(
            !handled,
            "status notification should not terminate streaming"
        );
        assert!(
            client.known_threads.is_empty(),
            "notLoaded for an unknown thread should not trigger a refresh read"
        );

        server.join().expect("websocket server should exit cleanly");
    }

    #[test]
    fn thread_closed_notification_removes_known_thread_summary() {
        let mut child = std::process::Command::new("cat")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("child should spawn");
        let mut client = CodexClient {
            transport: ClientTransport::Stdio {
                stdin: child.stdin.take(),
                stdout: std::io::BufReader::new(
                    child.stdout.take().expect("child stdout should exist"),
                ),
                child,
            },
            pending_notifications: VecDeque::new(),
            known_threads: BTreeMap::from([(
                "thread-1".to_string(),
                KnownThreadSummary {
                    name: Some("atlas".to_string()),
                    mode: ThreadMode::ResidentAssistant,
                    resident: true,
                    status: ThreadStatus::NotLoaded,
                },
            )]),
            command_approval_behavior: super::CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        };

        let handled = client.print_stream_notification(
            ServerNotification::ThreadClosed(ThreadClosedNotification {
                thread_id: "thread-1".to_string(),
            }),
            None,
            None,
        );
        assert!(
            !handled,
            "thread/closed notification should not terminate streaming"
        );
        assert!(
            !client.known_threads.contains_key("thread-1"),
            "thread/closed should evict cached thread summary"
        );
    }
}

fn thread_increment_elicitation(url: &str, thread_id: String) -> Result<()> {
    let endpoint = Endpoint::ConnectWs(url.to_string());
    let mut client = CodexClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let response =
        client.thread_increment_elicitation(ThreadIncrementElicitationParams { thread_id })?;
    println!("< thread/increment_elicitation response: {response:?}");

    Ok(())
}

fn thread_decrement_elicitation(url: &str, thread_id: String) -> Result<()> {
    let endpoint = Endpoint::ConnectWs(url.to_string());
    let mut client = CodexClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let response =
        client.thread_decrement_elicitation(ThreadDecrementElicitationParams { thread_id })?;
    println!("< thread/decrement_elicitation response: {response:?}");

    Ok(())
}

fn live_elicitation_timeout_pause(
    codex_bin: Option<PathBuf>,
    url: Option<String>,
    config_overrides: &[String],
    model: String,
    workspace: PathBuf,
    script: Option<PathBuf>,
    hold_seconds: u64,
) -> Result<()> {
    if cfg!(windows) {
        bail!("live-elicitation-timeout-pause currently requires a POSIX shell");
    }
    if hold_seconds <= 10 {
        bail!("--hold-seconds must be greater than 10 to exceed the unified exec timeout");
    }

    let mut _background_server = None;
    let websocket_url = match (codex_bin, url) {
        (Some(_), Some(_)) => bail!("--codex-bin and --url are mutually exclusive"),
        (Some(codex_bin), None) => {
            let server = BackgroundAppServer::spawn(&codex_bin, config_overrides)?;
            let websocket_url = server.url.clone();
            _background_server = Some(server);
            websocket_url
        }
        (None, Some(url)) => url,
        (None, None) => "ws://127.0.0.1:4222".to_string(),
    };

    let script_path = script.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("live_elicitation_hold.sh")
    });
    if !script_path.is_file() {
        bail!("helper script not found: {}", script_path.display());
    }

    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace `{}`", workspace.display()))?;
    let app_server_test_client_bin = std::env::current_exe()
        .context("failed to resolve codex-app-server-test-client binary path")?;
    let endpoint = Endpoint::ConnectWs(websocket_url.clone());
    let mut client = CodexClient::connect(&endpoint, &[])?;

    let initialize = client.initialize()?;
    println!("< initialize response: {initialize:?}");

    let mut thread_params = interactive_thread_start_params();
    thread_params.model = Some(model);
    let thread_response = client.thread_start(thread_params)?;
    println!("< thread/start response: {thread_response:?}");
    print_thread_response_summary("thread/start", &thread_response.thread);

    let thread_id = thread_response.thread.id;
    let command = format!(
        "APP_SERVER_URL={} APP_SERVER_TEST_CLIENT_BIN={} ELICITATION_HOLD_SECONDS={} sh {}",
        shell_quote(&websocket_url),
        shell_quote(&app_server_test_client_bin.display().to_string()),
        hold_seconds,
        shell_quote(&script_path.display().to_string()),
    );
    let prompt = format!(
        "Use the `exec_command` tool exactly once. Set its `cmd` field to the exact shell command below. Do not rewrite it, do not split it, do not call any other tool, do not set `yield_time_ms`, and wait for the command to finish before replying.\n\n{command}\n\nAfter the command finishes, reply with exactly `DONE`."
    );

    let started_at = Instant::now();
    let turn_response = client.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        input: vec![V2UserInput::Text {
            text: prompt,
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
        effort: Some(ReasoningEffort::High),
        cwd: Some(workspace),
        ..Default::default()
    })?;
    println!("< turn/start response: {turn_response:?}");

    let stream_result = client.stream_turn(&thread_id, &turn_response.turn.id);
    let elapsed = started_at.elapsed();

    let validation_result = (|| -> Result<()> {
        stream_result?;

        let helper_output = client
            .command_execution_outputs
            .iter()
            .find(|output| output.contains("[elicitation-hold]"))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("expected helper script markers in command output"))?;
        let minimum_elapsed = Duration::from_secs(hold_seconds.saturating_sub(1));

        if client.last_turn_status != Some(TurnStatus::Completed) {
            bail!(
                "expected completed turn, got {:?} (last error: {:?})",
                client.last_turn_status,
                client.last_turn_error_message
            );
        }
        if !client
            .command_execution_statuses
            .contains(&CommandExecutionStatus::Completed)
        {
            bail!(
                "expected a completed command execution, got {:?}",
                client.command_execution_statuses
            );
        }
        if !client.helper_done_seen || !helper_output.contains("[elicitation-hold] done") {
            bail!(
                "expected helper script completion marker in command output, got: {helper_output:?}"
            );
        }
        if !client.unexpected_items_before_helper_done.is_empty() {
            bail!(
                "turn started new items before helper completion: {:?}",
                client.unexpected_items_before_helper_done
            );
        }
        if client.turn_completed_before_helper_done {
            bail!("turn completed before helper script finished");
        }
        if elapsed < minimum_elapsed {
            bail!(
                "turn completed too quickly to prove timeout pause worked: elapsed={elapsed:?}, expected at least {minimum_elapsed:?}"
            );
        }

        Ok(())
    })();

    match client.thread_decrement_elicitation(ThreadDecrementElicitationParams {
        thread_id: thread_id.clone(),
    }) {
        Ok(response) => {
            println!("[cleanup] thread/decrement_elicitation response after harness: {response:?}");
        }
        Err(err) => {
            eprintln!("[cleanup] thread/decrement_elicitation ignored: {err:#}");
        }
    }

    validation_result?;

    println!(
        "[live elicitation timeout pause summary] thread_id={thread_id}, turn_id={}, elapsed={elapsed:?}, command_statuses={:?}",
        turn_response.turn.id, client.command_execution_statuses
    );

    Ok(())
}

fn ensure_dynamic_tools_unused(
    dynamic_tools: &Option<Vec<DynamicToolSpec>>,
    command: &str,
) -> Result<()> {
    if dynamic_tools.is_some() {
        bail!(
            "dynamic tools are only supported for v2 thread/start; remove --dynamic-tools for {command} or use send-message-v2"
        );
    }
    Ok(())
}

fn parse_dynamic_tools_arg(dynamic_tools: &Option<String>) -> Result<Option<Vec<DynamicToolSpec>>> {
    let Some(raw_arg) = dynamic_tools.as_deref() else {
        return Ok(None);
    };

    let raw_json = if let Some(path) = raw_arg.strip_prefix('@') {
        fs::read_to_string(Path::new(path))
            .with_context(|| format!("read dynamic tools file {path}"))?
    } else {
        raw_arg.to_string()
    };

    let value: Value = serde_json::from_str(&raw_json).context("parse dynamic tools JSON")?;
    let tools = match value {
        Value::Array(_) => serde_json::from_value(value).context("decode dynamic tools array")?,
        Value::Object(_) => vec![serde_json::from_value(value).context("decode dynamic tool")?],
        _ => bail!("dynamic tools JSON must be an object or array"),
    };

    Ok(Some(tools))
}

enum ClientTransport {
    Stdio {
        child: Child,
        stdin: Option<ChildStdin>,
        stdout: BufReader<ChildStdout>,
    },
    WebSocket {
        url: String,
        socket: Box<WebSocket<MaybeTlsStream<TcpStream>>>,
    },
}

struct CodexClient {
    transport: ClientTransport,
    pending_notifications: VecDeque<JSONRPCNotification>,
    known_threads: BTreeMap<String, KnownThreadSummary>,
    command_approval_behavior: CommandApprovalBehavior,
    command_approval_count: usize,
    command_approval_item_ids: Vec<String>,
    command_execution_statuses: Vec<CommandExecutionStatus>,
    command_execution_outputs: Vec<String>,
    command_output_stream: String,
    command_item_started: bool,
    helper_done_seen: bool,
    turn_completed_before_helper_done: bool,
    unexpected_items_before_helper_done: Vec<ThreadItem>,
    last_turn_status: Option<TurnStatus>,
    last_turn_error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct KnownThreadSummary {
    name: Option<String>,
    mode: ThreadMode,
    resident: bool,
    status: codex_app_server_protocol::ThreadStatus,
}

#[derive(Debug, Clone, Copy)]
enum CommandApprovalBehavior {
    AlwaysAccept,
    AbortOn(usize),
}

fn item_started_before_helper_done_is_unexpected(
    item: &ThreadItem,
    command_item_started: bool,
    helper_done_seen: bool,
) -> bool {
    if !command_item_started || helper_done_seen {
        return false;
    }

    !matches!(item, ThreadItem::UserMessage { .. })
}

impl CodexClient {
    fn connect(endpoint: &Endpoint, config_overrides: &[String]) -> Result<Self> {
        match endpoint {
            Endpoint::SpawnCodex(codex_bin) => Self::spawn_stdio(codex_bin, config_overrides),
            Endpoint::ConnectWs(url) => Self::connect_websocket(url),
        }
    }

    fn spawn_stdio(codex_bin: &Path, config_overrides: &[String]) -> Result<Self> {
        let codex_bin_display = codex_bin.display();
        let mut cmd = Command::new(codex_bin);
        if let Some(codex_bin_parent) = codex_bin.parent() {
            let mut path = OsString::from(codex_bin_parent.as_os_str());
            if let Some(existing_path) = std::env::var_os("PATH") {
                path.push(":");
                path.push(existing_path);
            }
            cmd.env("PATH", path);
        }
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }
        let mut codex_app_server = cmd
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{codex_bin_display}` app-server"))?;

        let stdin = codex_app_server
            .stdin
            .take()
            .context("codex app-server stdin unavailable")?;
        let stdout = codex_app_server
            .stdout
            .take()
            .context("codex app-server stdout unavailable")?;

        Ok(Self {
            transport: ClientTransport::Stdio {
                child: codex_app_server,
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
            },
            pending_notifications: VecDeque::new(),
            known_threads: BTreeMap::new(),
            command_approval_behavior: CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        })
    }

    fn connect_websocket(url: &str) -> Result<Self> {
        let parsed = Url::parse(url).with_context(|| format!("invalid websocket URL `{url}`"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let (socket, _response) = loop {
            match connect(parsed.as_str()) {
                Ok(result) => break result,
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(err).with_context(|| {
                            format!(
                                "failed to connect to websocket app-server at `{url}`; if no server is running, start one with `codex-app-server-test-client serve --listen {url}`"
                            )
                        });
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        };
        Ok(Self {
            transport: ClientTransport::WebSocket {
                url: url.to_string(),
                socket: Box::new(socket),
            },
            pending_notifications: VecDeque::new(),
            known_threads: BTreeMap::new(),
            command_approval_behavior: CommandApprovalBehavior::AlwaysAccept,
            command_approval_count: 0,
            command_approval_item_ids: Vec::new(),
            command_execution_statuses: Vec::new(),
            command_execution_outputs: Vec::new(),
            command_output_stream: String::new(),
            command_item_started: false,
            helper_done_seen: false,
            turn_completed_before_helper_done: false,
            unexpected_items_before_helper_done: Vec::new(),
            last_turn_status: None,
            last_turn_error_message: None,
        })
    }

    fn note_helper_output(&mut self, output: &str) {
        self.command_output_stream.push_str(output);
        if self
            .command_output_stream
            .contains("[elicitation-hold] done")
        {
            self.helper_done_seen = true;
        }
    }

    fn remember_thread(&mut self, thread: &AppServerThread) {
        self.known_threads.insert(
            thread.id.clone(),
            KnownThreadSummary {
                name: thread.name.clone(),
                mode: thread.mode,
                resident: thread.resident,
                status: thread.status.clone(),
            },
        );
    }

    fn remember_threads(&mut self, threads: &[AppServerThread]) {
        for thread in threads {
            self.remember_thread(thread);
        }
    }

    fn print_stream_notification(
        &mut self,
        server_notification: ServerNotification,
        thread_id: Option<&str>,
        turn_id: Option<&str>,
    ) -> bool {
        match server_notification {
            ServerNotification::ThreadStarted(payload) => {
                self.remember_thread(&payload.thread);
                if thread_id.is_none_or(|id| payload.thread.id == id) {
                    for line in thread_started_notification_lines(
                        &payload.thread,
                        /*include_raw_debug*/ thread_id.is_some(),
                    ) {
                        println!("{line}");
                    }
                }
                false
            }
            ServerNotification::ThreadStatusChanged(payload) => {
                let known_thread = self
                    .known_threads
                    .get_mut(&payload.thread_id)
                    .map(|thread| {
                        thread.status = payload.status.clone();
                        thread.clone()
                    });
                if thread_id.is_none_or(|id| payload.thread_id == id) {
                    for line in thread_status_changed_notification_lines(
                        &payload.thread_id,
                        &payload.status,
                        known_thread.as_ref(),
                    ) {
                        println!("{line}");
                    }
                }
                if known_thread.is_none()
                    && payload.status != codex_app_server_protocol::ThreadStatus::NotLoaded
                {
                    self.refresh_unknown_thread_summary(&payload.thread_id);
                }
                false
            }
            ServerNotification::ThreadClosed(ThreadClosedNotification {
                thread_id: closed_thread_id,
            }) => {
                let known_thread = self.known_threads.remove(&closed_thread_id);
                if thread_id.is_none_or(|id| closed_thread_id == id) {
                    for line in
                        thread_closed_notification_lines(&closed_thread_id, known_thread.as_ref())
                    {
                        println!("{line}");
                    }
                }
                false
            }
            ServerNotification::ThreadNameUpdated(payload) => {
                let known_thread = self
                    .known_threads
                    .get_mut(&payload.thread_id)
                    .map(|thread| {
                        thread.name = payload.thread_name.clone();
                        thread.clone()
                    });
                if thread_id.is_none_or(|id| payload.thread_id == id) {
                    for line in thread_name_updated_notification_lines(
                        &payload.thread_id,
                        payload.thread_name.as_deref(),
                        known_thread.as_ref(),
                    ) {
                        println!("{line}");
                    }
                }
                false
            }
            ServerNotification::TurnStarted(payload) => {
                if turn_id.is_some_and(|id| payload.turn.id == id) {
                    println!("< turn/started notification: {:?}", payload.turn.status);
                }
                false
            }
            ServerNotification::AgentMessageDelta(delta) => {
                print!("{}", delta.delta);
                std::io::stdout().flush().ok();
                false
            }
            ServerNotification::CommandExecutionOutputDelta(delta) => {
                self.note_helper_output(&delta.delta);
                print!("{}", delta.delta);
                std::io::stdout().flush().ok();
                false
            }
            ServerNotification::TerminalInteraction(delta) => {
                println!("[stdin sent: {}]", delta.stdin);
                std::io::stdout().flush().ok();
                false
            }
            ServerNotification::ItemStarted(payload) => {
                if matches!(payload.item, ThreadItem::CommandExecution { .. }) {
                    if self.command_item_started && !self.helper_done_seen {
                        self.unexpected_items_before_helper_done
                            .push(payload.item.clone());
                    }
                    self.command_item_started = true;
                } else if item_started_before_helper_done_is_unexpected(
                    &payload.item,
                    self.command_item_started,
                    self.helper_done_seen,
                ) {
                    self.unexpected_items_before_helper_done
                        .push(payload.item.clone());
                }
                println!("\n< item started: {:?}", payload.item);
                false
            }
            ServerNotification::ItemCompleted(payload) => {
                if let ThreadItem::CommandExecution {
                    status,
                    aggregated_output,
                    ..
                } = payload.item.clone()
                {
                    self.command_execution_statuses.push(status);
                    if let Some(aggregated_output) = aggregated_output {
                        self.note_helper_output(&aggregated_output);
                        self.command_execution_outputs.push(aggregated_output);
                    }
                }
                println!("< item completed: {:?}", payload.item);
                false
            }
            ServerNotification::TurnCompleted(payload) => {
                if turn_id.is_some_and(|id| payload.turn.id == id) {
                    self.last_turn_status = Some(payload.turn.status.clone());
                    if self.command_item_started && !self.helper_done_seen {
                        self.turn_completed_before_helper_done = true;
                    }
                    self.last_turn_error_message = payload
                        .turn
                        .error
                        .as_ref()
                        .map(|error| error.message.clone());
                    println!("\n< turn/completed notification: {:?}", payload.turn.status);
                    if payload.turn.status == TurnStatus::Failed
                        && let Some(error) = payload.turn.error
                    {
                        println!("[turn error] {}", error.message);
                    }
                    return true;
                }
                false
            }
            ServerNotification::McpToolCallProgress(payload) => {
                println!("< MCP tool progress: {}", payload.message);
                false
            }
            _ => {
                println!("[UNKNOWN SERVER NOTIFICATION] {server_notification:?}");
                false
            }
        }
    }

    fn initialize(&mut self) -> Result<InitializeResponse> {
        self.initialize_with_experimental_api(/*experimental_api*/ true)
    }

    fn initialize_with_experimental_api(
        &mut self,
        experimental_api: bool,
    ) -> Result<InitializeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-toy-app-server".to_string(),
                    title: Some("Codex Toy App Server".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api,
                    opt_out_notification_methods: Some(
                        NOTIFICATIONS_TO_OPT_OUT
                            .iter()
                            .map(|method| (*method).to_string())
                            .collect(),
                    ),
                }),
            },
        };

        let response: InitializeResponse = self.send_request(request, request_id, "initialize")?;

        // Complete the initialize handshake.
        let initialized = JSONRPCMessage::Notification(JSONRPCNotification {
            method: "initialized".to_string(),
            params: None,
        });
        self.write_jsonrpc_message(initialized)?;

        Ok(response)
    }

    fn thread_start(&mut self, params: ThreadStartParams) -> Result<ThreadStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadStartResponse =
            self.send_request(request, request_id, "thread/start")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_resume(&mut self, params: ThreadResumeParams) -> Result<ThreadResumeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadResumeResponse =
            self.send_request(request, request_id, "thread/resume")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_fork(&mut self, params: ThreadForkParams) -> Result<ThreadForkResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadFork {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadForkResponse = self.send_request(request, request_id, "thread/fork")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn turn_start(&mut self, params: TurnStartParams) -> Result<TurnStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "turn/start")
    }

    fn login_account_chatgpt(&mut self) -> Result<LoginAccountResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::LoginAccount {
            request_id: request_id.clone(),
            params: codex_app_server_protocol::LoginAccountParams::Chatgpt,
        };

        self.send_request(request, request_id, "account/login/start")
    }

    fn login_account_chatgpt_device_code(&mut self) -> Result<LoginAccountResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::LoginAccount {
            request_id: request_id.clone(),
            params: codex_app_server_protocol::LoginAccountParams::ChatgptDeviceCode,
        };

        self.send_request(request, request_id, "account/login/start")
    }

    fn get_account_rate_limits(&mut self) -> Result<GetAccountRateLimitsResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::GetAccountRateLimits {
            request_id: request_id.clone(),
            params: None,
        };

        self.send_request(request, request_id, "account/rateLimits/read")
    }

    fn model_list(&mut self, params: ModelListParams) -> Result<ModelListResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ModelList {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "model/list")
    }

    fn thread_list(&mut self, params: ThreadListParams) -> Result<ThreadListResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadList {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadListResponse = self.send_request(request, request_id, "thread/list")?;
        self.remember_threads(&response.data);
        Ok(response)
    }

    fn thread_read(&mut self, params: ThreadReadParams) -> Result<ThreadReadResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadRead {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadReadResponse = self.send_request(request, request_id, "thread/read")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_close(&mut self, params: ThreadCloseParams) -> Result<ThreadCloseResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadClose {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/close")
    }

    fn refresh_unknown_thread_summary(&mut self, thread_id: &str) {
        match self.thread_read(ThreadReadParams {
            thread_id: thread_id.to_string(),
            include_turns: false,
        }) {
            Ok(response) => println!(
                "{}",
                thread_response_summary_line("thread/status/changed refresh", &response.thread)
            ),
            Err(err) => {
                println!(
                    "< thread/status/changed refresh failed: thread_id={thread_id}, error={err:#}"
                );
            }
        }
    }

    fn thread_loaded_read(
        &mut self,
        params: ThreadLoadedReadParams,
    ) -> Result<ThreadLoadedReadResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadLoadedRead {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadLoadedReadResponse =
            self.send_request(request, request_id, "thread/loaded/read")?;
        self.remember_threads(&response.data);
        Ok(response)
    }

    fn thread_loaded_list(
        &mut self,
        params: ThreadLoadedListParams,
    ) -> Result<ThreadLoadedListResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadLoadedList {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/loaded/list")
    }

    fn thread_metadata_update(
        &mut self,
        params: ThreadMetadataUpdateParams,
    ) -> Result<ThreadMetadataUpdateResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadMetadataUpdate {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadMetadataUpdateResponse =
            self.send_request(request, request_id, "thread/metadata/update")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_unarchive(
        &mut self,
        params: ThreadUnarchiveParams,
    ) -> Result<ThreadUnarchiveResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadUnarchive {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadUnarchiveResponse =
            self.send_request(request, request_id, "thread/unarchive")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_rollback(&mut self, params: ThreadRollbackParams) -> Result<ThreadRollbackResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadRollback {
            request_id: request_id.clone(),
            params,
        };

        let response: ThreadRollbackResponse =
            self.send_request(request, request_id, "thread/rollback")?;
        self.remember_thread(&response.thread);
        Ok(response)
    }

    fn thread_increment_elicitation(
        &mut self,
        params: ThreadIncrementElicitationParams,
    ) -> Result<ThreadIncrementElicitationResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadIncrementElicitation {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/increment_elicitation")
    }

    fn thread_decrement_elicitation(
        &mut self,
        params: ThreadDecrementElicitationParams,
    ) -> Result<ThreadDecrementElicitationResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadDecrementElicitation {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/decrement_elicitation")
    }

    fn wait_for_account_login_completion(
        &mut self,
        expected_login_id: &str,
    ) -> Result<AccountLoginCompletedNotification> {
        loop {
            let notification = self.next_notification()?;

            if let Ok(server_notification) = ServerNotification::try_from(notification) {
                match server_notification {
                    ServerNotification::AccountLoginCompleted(completion) => {
                        if completion.login_id.as_deref() == Some(expected_login_id) {
                            return Ok(completion);
                        }

                        println!(
                            "[ignoring account/login/completed for unexpected login_id: {:?}]",
                            completion.login_id
                        );
                    }
                    ServerNotification::AccountRateLimitsUpdated(snapshot) => {
                        println!("< accountRateLimitsUpdated notification: {snapshot:?}");
                    }
                    _ => {}
                }
            }
        }
    }

    fn stream_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<()> {
        loop {
            let notification = self.next_notification()?;

            let Ok(server_notification) = ServerNotification::try_from(notification) else {
                continue;
            };

            if self.print_stream_notification(server_notification, Some(thread_id), Some(turn_id)) {
                break;
            }
        }

        Ok(())
    }

    fn stream_notifications_forever(&mut self) -> Result<()> {
        loop {
            let notification = self.next_notification()?;

            let Ok(server_notification) = ServerNotification::try_from(notification) else {
                continue;
            };

            let _ = self.print_stream_notification(server_notification, None, None);
        }
    }

    fn send_request<T>(
        &mut self,
        request: ClientRequest,
        request_id: RequestId,
        method: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let request_span = info_span!(
            "app_server_test_client.request",
            otel.kind = "client",
            otel.name = method,
            rpc.system = "jsonrpc",
            rpc.method = method,
            rpc.request_id = ?request_id,
        );
        request_span.in_scope(|| {
            self.write_request(&request)?;
            self.wait_for_response(request_id, method)
        })
    }

    fn write_request(&mut self, request: &ClientRequest) -> Result<()> {
        let request_value = serde_json::to_value(request)?;
        let mut request: JSONRPCRequest = serde_json::from_value(request_value)
            .context("client request was not a valid JSON-RPC request")?;
        request.trace = current_span_w3c_trace_context();
        let request_json = serde_json::to_string(&request)?;
        let request_pretty = serde_json::to_string_pretty(&request)?;
        print_multiline_with_prefix("> ", &request_pretty);
        self.write_payload(&request_json)
    }

    fn wait_for_response<T>(&mut self, request_id: RequestId, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        loop {
            let message = self.read_jsonrpc_message()?;

            match message {
                JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                    if id == request_id {
                        return serde_json::from_value(result)
                            .with_context(|| format!("{method} response missing payload"));
                    }
                }
                JSONRPCMessage::Error(err) => {
                    if err.id == request_id {
                        bail!("{method} failed: {err:?}");
                    }
                }
                JSONRPCMessage::Notification(notification) => {
                    self.pending_notifications.push_back(notification);
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request)?;
                }
            }
        }
    }

    fn next_notification(&mut self) -> Result<JSONRPCNotification> {
        if let Some(notification) = self.pending_notifications.pop_front() {
            return Ok(notification);
        }

        loop {
            let message = self.read_jsonrpc_message()?;

            match message {
                JSONRPCMessage::Notification(notification) => return Ok(notification),
                JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {
                    // No outstanding requests, so ignore stray responses/errors for now.
                    continue;
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request)?;
                }
            }
        }
    }

    fn read_jsonrpc_message(&mut self) -> Result<JSONRPCMessage> {
        loop {
            let raw = self.read_payload()?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parsed: Value =
                serde_json::from_str(trimmed).context("response was not valid JSON-RPC")?;
            let pretty = serde_json::to_string_pretty(&parsed)?;
            print_multiline_with_prefix("< ", &pretty);
            let message: JSONRPCMessage = serde_json::from_value(parsed)
                .context("response was not a valid JSON-RPC message")?;
            return Ok(message);
        }
    }

    fn request_id(&self) -> RequestId {
        RequestId::String(Uuid::new_v4().to_string())
    }

    fn handle_server_request(&mut self, request: JSONRPCRequest) -> Result<()> {
        let server_request = ServerRequest::try_from(request)
            .context("failed to deserialize ServerRequest from JSONRPCRequest")?;

        match server_request {
            ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                self.handle_command_execution_request_approval(request_id, params)?;
            }
            ServerRequest::FileChangeRequestApproval { request_id, params } => {
                self.approve_file_change_request(request_id, params)?;
            }
            ServerRequest::PermissionsRequestApproval { request_id, params } => {
                self.approve_permissions_request(request_id, params)?;
            }
            ServerRequest::ToolRequestUserInput { request_id, params } => {
                self.answer_request_user_input(request_id, params)?;
            }
            ServerRequest::McpServerElicitationRequest { request_id, params } => {
                self.resolve_mcp_elicitation_request(request_id, params)?;
            }
            other => {
                bail!("received unsupported server request: {other:?}");
            }
        }

        Ok(())
    }

    fn handle_command_execution_request_approval(
        &mut self,
        request_id: RequestId,
        params: CommandExecutionRequestApprovalParams,
    ) -> Result<()> {
        let CommandExecutionRequestApprovalParams {
            thread_id,
            turn_id,
            item_id,
            approval_id,
            reason,
            network_approval_context,
            command,
            cwd,
            command_actions,
            additional_permissions,
            proposed_execpolicy_amendment,
            proposed_network_policy_amendments,
            available_decisions,
        } = params;

        println!(
            "\n< commandExecution approval requested for thread {thread_id}, turn {turn_id}, item {item_id}, approval {}",
            approval_id.as_deref().unwrap_or("<none>")
        );
        self.command_approval_count += 1;
        self.command_approval_item_ids.push(item_id.clone());
        if let Some(reason) = reason.as_deref() {
            println!("< reason: {reason}");
        }
        if let Some(network_approval_context) = network_approval_context.as_ref() {
            println!("< network approval context: {network_approval_context:?}");
        }
        if let Some(available_decisions) = available_decisions.as_ref() {
            println!("< available decisions: {available_decisions:?}");
        }
        if let Some(command) = command.as_deref() {
            println!("< command: {command}");
        }
        if let Some(cwd) = cwd.as_ref() {
            println!("< cwd: {}", cwd.display());
        }
        if let Some(command_actions) = command_actions.as_ref()
            && !command_actions.is_empty()
        {
            println!("< command actions: {command_actions:?}");
        }
        if let Some(additional_permissions) = additional_permissions.as_ref() {
            println!("< additional permissions: {additional_permissions:?}");
        }
        if let Some(execpolicy_amendment) = proposed_execpolicy_amendment.as_ref() {
            println!("< proposed execpolicy amendment: {execpolicy_amendment:?}");
        }
        if let Some(network_policy_amendments) = proposed_network_policy_amendments.as_ref() {
            println!("< proposed network policy amendments: {network_policy_amendments:?}");
        }

        let decision = match self.command_approval_behavior {
            CommandApprovalBehavior::AlwaysAccept => CommandExecutionApprovalDecision::Accept,
            CommandApprovalBehavior::AbortOn(index) if self.command_approval_count == index => {
                CommandExecutionApprovalDecision::Cancel
            }
            CommandApprovalBehavior::AbortOn(_) => CommandExecutionApprovalDecision::Accept,
        };
        let response = CommandExecutionRequestApprovalResponse {
            decision: decision.clone(),
        };
        self.send_server_request_response(request_id, &response)?;
        println!(
            "< commandExecution decision for approval #{} on item {item_id}: {:?}",
            self.command_approval_count, decision
        );
        Ok(())
    }

    fn approve_file_change_request(
        &mut self,
        request_id: RequestId,
        params: FileChangeRequestApprovalParams,
    ) -> Result<()> {
        let FileChangeRequestApprovalParams {
            thread_id,
            turn_id,
            item_id,
            reason,
            grant_root,
        } = params;

        println!(
            "\n< fileChange approval requested for thread {thread_id}, turn {turn_id}, item {item_id}"
        );
        if let Some(reason) = reason.as_deref() {
            println!("< reason: {reason}");
        }
        if let Some(grant_root) = grant_root.as_deref() {
            println!("< grant root: {}", grant_root.display());
        }

        let response = FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::Accept,
        };
        self.send_server_request_response(request_id, &response)?;
        println!("< approved fileChange request for item {item_id}");
        Ok(())
    }

    fn approve_permissions_request(
        &mut self,
        request_id: RequestId,
        params: PermissionsRequestApprovalParams,
    ) -> Result<()> {
        let PermissionsRequestApprovalParams {
            thread_id,
            turn_id,
            item_id,
            reason,
            permissions,
        } = params;

        println!(
            "\n< permissions approval requested for thread {thread_id}, turn {turn_id}, item {item_id}"
        );
        if let Some(reason) = reason.as_deref() {
            println!("< reason: {reason}");
        }
        println!("< requested permissions: {permissions:?}");

        let response = PermissionsRequestApprovalResponse {
            permissions: granted_permission_profile_from_request(permissions),
            scope: PermissionGrantScope::Turn,
        };
        self.send_server_request_response(request_id, &response)?;
        println!("< granted permissions request for item {item_id}");
        Ok(())
    }

    fn answer_request_user_input(
        &mut self,
        request_id: RequestId,
        params: ToolRequestUserInputParams,
    ) -> Result<()> {
        let ToolRequestUserInputParams {
            thread_id,
            turn_id,
            item_id,
            questions,
        } = params;

        println!(
            "\n< request_user_input requested for thread {thread_id}, turn {turn_id}, item {item_id}"
        );
        println!("< questions: {questions:?}");

        let response = ToolRequestUserInputResponse {
            answers: default_user_input_answers(&questions),
        };
        self.send_server_request_response(request_id, &response)?;
        println!("< answered request_user_input for item {item_id}");
        Ok(())
    }

    fn resolve_mcp_elicitation_request(
        &mut self,
        request_id: RequestId,
        params: codex_app_server_protocol::McpServerElicitationRequestParams,
    ) -> Result<()> {
        let thread_id = params.thread_id;
        let turn_id = params.turn_id;
        let server_name = params.server_name;
        let request = params.request;

        println!(
            "\n< MCP elicitation requested for thread {thread_id}, turn {}, server {server_name}",
            turn_id.as_deref().unwrap_or("-")
        );
        println!("< elicitation request: {request:?}");

        let response = McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Cancel,
            content: None,
            meta: None,
        };
        self.send_server_request_response(request_id, &response)?;
        println!("< cancelled MCP elicitation for server {server_name}");
        Ok(())
    }

    fn send_server_request_response<T>(&mut self, request_id: RequestId, response: &T) -> Result<()>
    where
        T: Serialize,
    {
        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result: serde_json::to_value(response)?,
        });
        self.write_jsonrpc_message(message)
    }

    fn write_jsonrpc_message(&mut self, message: JSONRPCMessage) -> Result<()> {
        let payload = serde_json::to_string(&message)?;
        let pretty = serde_json::to_string_pretty(&message)?;
        print_multiline_with_prefix("> ", &pretty);
        self.write_payload(&payload)
    }

    fn write_payload(&mut self, payload: &str) -> Result<()> {
        match &mut self.transport {
            ClientTransport::Stdio { stdin, .. } => {
                if let Some(stdin) = stdin.as_mut() {
                    writeln!(stdin, "{payload}")?;
                    stdin
                        .flush()
                        .context("failed to flush payload to codex app-server")?;
                    return Ok(());
                }
                bail!("codex app-server stdin closed")
            }
            ClientTransport::WebSocket { socket, url } => {
                socket
                    .send(Message::Text(payload.to_string().into()))
                    .with_context(|| format!("failed to write websocket message to `{url}`"))?;
                Ok(())
            }
        }
    }

    fn read_payload(&mut self) -> Result<String> {
        match &mut self.transport {
            ClientTransport::Stdio { stdout, .. } => {
                let mut response_line = String::new();
                let bytes = stdout
                    .read_line(&mut response_line)
                    .context("failed to read from codex app-server")?;
                if bytes == 0 {
                    bail!("codex app-server closed stdout");
                }
                Ok(response_line)
            }
            ClientTransport::WebSocket { socket, url } => loop {
                let frame = socket
                    .read()
                    .with_context(|| format!("failed to read websocket message from `{url}`"))?;
                match frame {
                    Message::Text(text) => return Ok(text.to_string()),
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => continue,
                    Message::Close(_) => {
                        bail!("websocket app-server at `{url}` closed the connection")
                    }
                    Message::Frame(_) => continue,
                }
            },
        }
    }
}

fn granted_permission_profile_from_request(
    value: codex_app_server_protocol::RequestPermissionProfile,
) -> codex_app_server_protocol::GrantedPermissionProfile {
    codex_app_server_protocol::GrantedPermissionProfile {
        network: value.network.map(|network| {
            codex_app_server_protocol::AdditionalNetworkPermissions {
                enabled: network.enabled,
            }
        }),
        file_system: value.file_system.map(|file_system| {
            codex_app_server_protocol::AdditionalFileSystemPermissions {
                read: file_system.read,
                write: file_system.write,
            }
        }),
    }
}

fn default_user_input_answers(
    questions: &[ToolRequestUserInputQuestion],
) -> std::collections::HashMap<String, ToolRequestUserInputAnswer> {
    questions
        .iter()
        .map(|question| {
            let answer = question
                .options
                .as_ref()
                .and_then(|options| options.first())
                .map(|option| option.label.clone())
                .unwrap_or_default();
            (
                question.id.clone(),
                ToolRequestUserInputAnswer {
                    answers: vec![answer],
                },
            )
        })
        .collect()
}

fn print_multiline_with_prefix(prefix: &str, payload: &str) {
    for line in payload.lines() {
        println!("{prefix}{line}");
    }
}

struct TestClientTracing {
    _otel_provider: Option<OtelProvider>,
    traces_enabled: bool,
}

impl TestClientTracing {
    async fn initialize(config_overrides: &[String]) -> Result<Self> {
        let cli_kv_overrides = CliConfigOverrides {
            raw_overrides: config_overrides.to_vec(),
        }
        .parse_overrides()
        .map_err(|e| anyhow::anyhow!("error parsing -c overrides: {e}"))?;
        let config = Config::load_with_cli_overrides(cli_kv_overrides)
            .await
            .context("error loading config")?;
        let otel_provider = codex_core::otel_init::build_provider(
            &config,
            env!("CARGO_PKG_VERSION"),
            Some(OTEL_SERVICE_NAME),
            DEFAULT_ANALYTICS_ENABLED,
        )
        .map_err(|e| anyhow::anyhow!("error loading otel config: {e}"))?;
        let traces_enabled = otel_provider
            .as_ref()
            .and_then(|provider| provider.tracer_provider.as_ref())
            .is_some();
        if let Some(provider) = otel_provider.as_ref()
            && traces_enabled
        {
            let _ = tracing_subscriber::registry()
                .with(provider.tracing_layer())
                .try_init();
        }
        Ok(Self {
            traces_enabled,
            _otel_provider: otel_provider,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TraceSummary {
    Enabled { url: String },
    Disabled,
}

impl TraceSummary {
    fn capture(traces_enabled: bool) -> Self {
        if !traces_enabled {
            return Self::Disabled;
        }
        current_span_w3c_trace_context()
            .as_ref()
            .and_then(trace_url_from_context)
            .map_or(Self::Disabled, |url| Self::Enabled { url })
    }
}

fn trace_url_from_context(trace: &W3cTraceContext) -> Option<String> {
    let traceparent = trace.traceparent.as_deref()?;
    let mut parts = traceparent.split('-');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(_version), Some(trace_id), Some(_span_id), Some(_trace_flags))
            if trace_id.len() == 32 =>
        {
            Some(format!("go/trace/{trace_id}"))
        }
        _ => None,
    }
}

fn print_trace_summary(trace_summary: &TraceSummary) {
    println!("\n[Datadog trace]");
    match trace_summary {
        TraceSummary::Enabled { url } => println!("{url}\n"),
        TraceSummary::Disabled => println!("{TRACE_DISABLED_MESSAGE}\n"),
    }
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        let ClientTransport::Stdio { child, stdin, .. } = &mut self.transport else {
            return;
        };

        let _ = stdin.take();

        if let Ok(Some(status)) = child.try_wait() {
            println!("[codex app-server exited: {status}]");
            return;
        }

        let deadline = SystemTime::now() + APP_SERVER_GRACEFUL_SHUTDOWN_TIMEOUT;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                println!("[codex app-server exited: {status}]");
                return;
            }

            if SystemTime::now() >= deadline {
                break;
            }

            thread::sleep(APP_SERVER_GRACEFUL_SHUTDOWN_POLL_INTERVAL);
        }

        let _ = child.kill();
        let _ = child.wait();
    }
}
