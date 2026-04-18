#![allow(clippy::expect_used)]
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use anyhow::Context;
use anyhow::Result;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::PermissionGrantScope;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use codex_app_server_protocol::all_thread_source_kinds;
use serde::Serialize;

use crate::output::Output;
use crate::reader::ReaderRequestSink;
use crate::reader::start_reader;
use crate::state::KnownThread;
use crate::state::PendingRequest;
use crate::state::ReaderEvent;
use crate::state::State;

pub struct ThreadConnection {
    pub thread_id: String,
    pub thread_name: Option<String>,
    pub thread_mode: ThreadMode,
}

pub struct AppServerClient {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout: Option<BufReader<ChildStdout>>,
    next_request_id: Arc<AtomicI64>,
    state: Arc<Mutex<State>>,
    output: Output,
    filtered_output: bool,
}

impl AppServerClient {
    pub fn spawn(
        codex_bin: &str,
        config_overrides: &[String],
        output: Output,
        filtered_output: bool,
    ) -> Result<Self> {
        let mut cmd = Command::new(codex_bin);
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }

        let mut child = cmd
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{codex_bin}` app-server"))?;

        let stdin = child
            .stdin
            .take()
            .context("codex app-server stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("codex app-server stdout unavailable")?;

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(Some(stdin))),
            stdout: Some(BufReader::new(stdout)),
            next_request_id: Arc::new(AtomicI64::new(1)),
            state: Arc::new(Mutex::new(State::default())),
            output,
            filtered_output,
        })
    }

    pub fn initialize(&mut self) -> Result<()> {
        let request_id = self.next_request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: codex_app_server_protocol::InitializeParams {
                client_info: ClientInfo {
                    name: "debug-client".to_string(),
                    title: Some("Debug Client".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api: true,
                    opt_out_notification_methods: None,
                }),
            },
        };

        self.send(&request)?;
        let response = self.read_until_response(&request_id)?;
        let _parsed: codex_app_server_protocol::InitializeResponse =
            serde_json::from_value(response.result).context("decode initialize response")?;
        let initialized = ClientNotification::Initialized;
        self.send(&initialized)?;
        Ok(())
    }

    pub fn start_thread(&mut self, params: ThreadStartParams) -> Result<ThreadConnection> {
        let request_id = self.next_request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };
        self.send(&request)?;
        let response = self.read_until_response(&request_id)?;
        let parsed: ThreadStartResponse =
            serde_json::from_value(response.result).context("decode thread/start response")?;
        let thread_id = parsed.thread.id;
        let thread_name = parsed.thread.name;
        let thread_mode = parsed.thread.mode;
        let thread_status = parsed.thread.status;
        self.set_thread_id(thread_id.clone());
        self.remember_thread(
            thread_id.clone(),
            thread_name.clone(),
            thread_mode,
            thread_status,
        );
        Ok(ThreadConnection {
            thread_id,
            thread_name,
            thread_mode,
        })
    }

    pub fn resume_thread(&mut self, params: ThreadResumeParams) -> Result<ThreadConnection> {
        let request_id = self.next_request_id();
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };
        self.send(&request)?;
        let response = self.read_until_response(&request_id)?;
        let parsed: ThreadResumeResponse =
            serde_json::from_value(response.result).context("decode thread/resume response")?;
        let thread_id = parsed.thread.id;
        let thread_name = parsed.thread.name;
        let thread_mode = parsed.thread.mode;
        let thread_status = parsed.thread.status;
        self.set_thread_id(thread_id.clone());
        self.remember_thread(
            thread_id.clone(),
            thread_name.clone(),
            thread_mode,
            thread_status,
        );
        Ok(ThreadConnection {
            thread_id,
            thread_name,
            thread_mode,
        })
    }

    pub fn request_thread_start(&self, params: ThreadStartParams) -> Result<RequestId> {
        let request_id = self.next_request_id();
        self.track_pending(request_id.clone(), PendingRequest::Start);
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn request_thread_resume(&self, params: ThreadResumeParams) -> Result<RequestId> {
        let request_id = self.next_request_id();
        self.track_pending(request_id.clone(), PendingRequest::Resume);
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn request_thread_list(&self, cursor: Option<String>) -> Result<RequestId> {
        let request_id = self.next_request_id();
        self.track_pending(request_id.clone(), PendingRequest::List);
        let request = ClientRequest::ThreadList {
            request_id: request_id.clone(),
            params: ThreadListParams {
                cursor,
                limit: None,
                sort_key: None,
                model_providers: None,
                source_kinds: Some(all_thread_source_kinds()),
                archived: None,
                cwd: None,
                search_term: None,
            },
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn request_thread_loaded_list(&self, cursor: Option<String>) -> Result<RequestId> {
        let request_id = self.next_request_id();
        self.track_pending(request_id.clone(), PendingRequest::LoadedList);
        let request = ClientRequest::ThreadLoadedList {
            request_id: request_id.clone(),
            params: ThreadLoadedListParams {
                cursor,
                limit: None,
                model_providers: None,
                source_kinds: Some(all_thread_source_kinds()),
                cwd: None,
            },
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn request_thread_loaded_read(&self, cursor: Option<String>) -> Result<RequestId> {
        let request_id = self.next_request_id();
        self.track_pending(request_id.clone(), PendingRequest::LoadedRead);
        let request = ClientRequest::ThreadLoadedRead {
            request_id: request_id.clone(),
            params: ThreadLoadedReadParams {
                cursor,
                limit: None,
                model_providers: None,
                source_kinds: Some(all_thread_source_kinds()),
                cwd: None,
            },
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn send_turn(&self, thread_id: &str, text: String) -> Result<RequestId> {
        let request_id = self.next_request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params: TurnStartParams {
                thread_id: thread_id.to_string(),
                input: vec![UserInput::Text {
                    text,
                    // Debug client sends plain text with no UI markup spans.
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        };
        self.send(&request)?;
        Ok(request_id)
    }

    pub fn start_reader(
        &mut self,
        events: Sender<ReaderEvent>,
        auto_approve: bool,
        filtered_output: bool,
    ) -> Result<()> {
        let stdout = self.stdout.take().context("reader already started")?;
        start_reader(
            stdout,
            ReaderRequestSink {
                stdin: Arc::clone(&self.stdin),
                next_request_id: Arc::clone(&self.next_request_id),
            },
            Arc::clone(&self.state),
            events,
            self.output.clone(),
            auto_approve,
            filtered_output,
        );
        Ok(())
    }

    pub fn thread_id(&self) -> Option<String> {
        let state = self.state.lock().expect("state lock poisoned");
        state.thread_id.clone()
    }

    pub fn set_thread_id(&self, thread_id: String) {
        let mut state = self.state.lock().expect("state lock poisoned");
        state.thread_id = Some(thread_id);
    }

    pub fn use_thread(&self, thread_id: String) -> Option<KnownThread> {
        let mut state = self.state.lock().expect("state lock poisoned");
        let known_thread = state
            .thread_summaries
            .get(&thread_id)
            .map(known_thread_from_summary);
        state.thread_id = Some(thread_id);
        known_thread
    }

    pub fn known_thread(&self, thread_id: &str) -> Option<KnownThread> {
        let state = self.state.lock().expect("state lock poisoned");
        state
            .thread_summaries
            .get(thread_id)
            .map(known_thread_from_summary)
    }

    pub fn shutdown(&mut self) {
        if let Ok(mut stdin) = self.stdin.lock() {
            let _ = stdin.take();
        }
        let _ = self.child.wait();
    }

    fn track_pending(&self, request_id: RequestId, kind: PendingRequest) {
        let mut state = self.state.lock().expect("state lock poisoned");
        state.pending.insert(request_id, kind);
    }

    fn remember_thread(
        &self,
        thread_id: String,
        thread_name: Option<String>,
        thread_mode: ThreadMode,
        thread_status: ThreadStatus,
    ) {
        let mut state = self.state.lock().expect("state lock poisoned");
        state.thread_summaries.upsert_thread(thread_summary(
            thread_id,
            thread_name,
            thread_mode,
            thread_status,
        ));
    }

    fn next_request_id(&self) -> RequestId {
        let id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        RequestId::Integer(id)
    }

    fn send<T: Serialize>(&self, value: &T) -> Result<()> {
        let json = serde_json::to_string(value).context("serialize message")?;
        let mut line = json;
        line.push('\n');
        let mut stdin = self.stdin.lock().expect("stdin lock poisoned");
        let Some(stdin) = stdin.as_mut() else {
            anyhow::bail!("stdin already closed");
        };
        stdin.write_all(line.as_bytes()).context("write message")?;
        stdin.flush().context("flush message")?;
        Ok(())
    }

    fn read_until_response(&mut self, request_id: &RequestId) -> Result<JSONRPCResponse> {
        let stdin = Arc::clone(&self.stdin);
        let output = self.output.clone();
        let reader = self.stdout.as_mut().context("stdout missing")?;
        let mut buffer = String::new();

        loop {
            buffer.clear();
            let bytes = reader
                .read_line(&mut buffer)
                .context("read server output")?;
            if bytes == 0 {
                anyhow::bail!("server closed stdout while awaiting response {request_id:?}");
            }

            let line = buffer.trim_end_matches(['\n', '\r']);
            if !line.is_empty() && !self.filtered_output {
                let _ = output.server_line(line);
            }

            let message = match serde_json::from_str::<JSONRPCMessage>(line) {
                Ok(message) => message,
                Err(_) => continue,
            };

            match message {
                JSONRPCMessage::Response(response) => {
                    if &response.id == request_id {
                        return Ok(response);
                    }
                }
                JSONRPCMessage::Request(request) => {
                    let _ = handle_server_request(request, &stdin);
                }
                _ => {}
            }
        }
    }
}

fn thread_summary(
    thread_id: String,
    thread_name: Option<String>,
    thread_mode: ThreadMode,
    thread_status: ThreadStatus,
) -> Thread {
    Thread {
        id: thread_id,
        forked_from_id: None,
        preview: String::new(),
        ephemeral: false,
        model_provider: String::new(),
        created_at: 0,
        updated_at: 0,
        status: thread_status,
        mode: thread_mode,
        resident: thread_mode == ThreadMode::ResidentAssistant,
        path: None,
        cwd: std::path::PathBuf::new(),
        cli_version: String::new(),
        source: codex_app_server_protocol::SessionSource::Cli,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: thread_name,
        turns: Vec::new(),
    }
}

fn known_thread_from_summary(thread: &Thread) -> KnownThread {
    KnownThread {
        thread_id: thread.id.clone(),
        thread_name: thread.name.clone(),
        thread_mode: thread.mode,
        thread_status: thread.status.clone(),
    }
}

fn handle_server_request(
    request: JSONRPCRequest,
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
) -> Result<()> {
    let Ok(server_request) = codex_app_server_protocol::ServerRequest::try_from(request) else {
        return Ok(());
    };

    match server_request {
        codex_app_server_protocol::ServerRequest::CommandExecutionRequestApproval {
            request_id,
            ..
        } => {
            let response = codex_app_server_protocol::CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Decline,
            };
            send_jsonrpc_response(stdin, request_id, response)
        }
        codex_app_server_protocol::ServerRequest::FileChangeRequestApproval {
            request_id, ..
        } => {
            let response = codex_app_server_protocol::FileChangeRequestApprovalResponse {
                decision: FileChangeApprovalDecision::Decline,
            };
            send_jsonrpc_response(stdin, request_id, response)
        }
        codex_app_server_protocol::ServerRequest::PermissionsRequestApproval {
            request_id,
            params,
        } => {
            let response = PermissionsRequestApprovalResponse {
                permissions: denied_permissions_response(&params.permissions),
                scope: PermissionGrantScope::Turn,
            };
            send_jsonrpc_response(stdin, request_id, response)
        }
        codex_app_server_protocol::ServerRequest::ToolRequestUserInput { request_id, params } => {
            let response = ToolRequestUserInputResponse {
                answers: default_user_input_answers(&params.questions),
            };
            send_jsonrpc_response(stdin, request_id, response)
        }
        codex_app_server_protocol::ServerRequest::McpServerElicitationRequest {
            request_id,
            ..
        } => {
            let response = McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Cancel,
                content: None,
                meta: None,
            };
            send_jsonrpc_response(stdin, request_id, response)
        }
        _ => Ok(()),
    }
}

fn denied_permissions_response(
    _request: &codex_app_server_protocol::RequestPermissionProfile,
) -> codex_app_server_protocol::GrantedPermissionProfile {
    codex_app_server_protocol::GrantedPermissionProfile {
        network: None,
        file_system: None,
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

fn send_jsonrpc_response<T: Serialize>(
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    request_id: RequestId,
    response: T,
) -> Result<()> {
    let result = serde_json::to_value(response).context("serialize response")?;
    let message = JSONRPCMessage::Response(JSONRPCResponse {
        id: request_id,
        result,
    });
    send_with_stdin(stdin, &message)
}

fn send_with_stdin<T: Serialize>(stdin: &Arc<Mutex<Option<ChildStdin>>>, value: &T) -> Result<()> {
    let json = serde_json::to_string(value).context("serialize message")?;
    let mut line = json;
    line.push('\n');
    let mut stdin = stdin.lock().expect("stdin lock poisoned");
    let Some(stdin) = stdin.as_mut() else {
        anyhow::bail!("stdin already closed");
    };
    stdin.write_all(line.as_bytes()).context("write message")?;
    stdin.flush().context("flush message")?;
    Ok(())
}

pub fn build_thread_start_params(
    approval_policy: AskForApproval,
    model: Option<String>,
    model_provider: Option<String>,
    cwd: Option<String>,
) -> ThreadStartParams {
    ThreadStartParams {
        mode: Some(ThreadMode::Interactive),
        model,
        model_provider,
        cwd,
        approval_policy: Some(approval_policy),
        experimental_raw_events: false,
        ..Default::default()
    }
}

pub fn build_thread_resume_params(
    thread_id: String,
    mode: Option<ThreadMode>,
    approval_policy: AskForApproval,
    model: Option<String>,
    model_provider: Option<String>,
    cwd: Option<String>,
) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id,
        mode,
        model,
        model_provider,
        cwd,
        approval_policy: Some(approval_policy),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::AppServerClient;
    use super::all_thread_source_kinds;
    use super::build_thread_resume_params;
    use super::build_thread_start_params;
    use super::default_user_input_answers;
    use super::denied_permissions_response;
    use super::thread_summary;
    use crate::output::Output;
    use crate::state::KnownThread;
    use crate::state::State;
    use codex_app_server_client::ThreadSummaryTracker;
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ToolRequestUserInputOption;
    use codex_app_server_protocol::ToolRequestUserInputQuestion;
    use codex_app_server_protocol::all_thread_source_kinds as protocol_all_thread_source_kinds;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicI64;

    fn client_with_known_threads(known_threads: Vec<KnownThread>) -> AppServerClient {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat should spawn for debug-client state test");

        let stdin = child.stdin.take().expect("stdin should be piped");
        let stdout = child.stdout.take().expect("stdout should be piped");

        AppServerClient {
            child,
            stdin: Arc::new(Mutex::new(Some(stdin))),
            stdout: Some(std::io::BufReader::new(stdout)),
            next_request_id: Arc::new(AtomicI64::new(1)),
            state: Arc::new(Mutex::new({
                let mut state = State {
                    pending: Default::default(),
                    thread_id: None,
                    thread_summaries: ThreadSummaryTracker::new(),
                };
                for known_thread in known_threads {
                    state.thread_summaries.upsert_thread(thread_summary(
                        known_thread.thread_id,
                        known_thread.thread_name,
                        known_thread.thread_mode,
                        known_thread.thread_status,
                    ));
                }
                state
            })),
            output: Output::new(),
            filtered_output: false,
        }
    }

    #[test]
    fn all_thread_source_kinds_covers_interactive_and_non_interactive_sources() {
        assert_eq!(
            all_thread_source_kinds(),
            protocol_all_thread_source_kinds()
        );
    }

    #[test]
    fn use_thread_returns_cached_mode_and_status_when_known() {
        let known_thread = KnownThread {
            thread_id: "thread-1".to_string(),
            thread_name: Some("atlas".to_string()),
            thread_mode: codex_app_server_protocol::ThreadMode::ResidentAssistant,
            thread_status: ThreadStatus::Active {
                active_flags: vec![codex_app_server_protocol::ThreadActiveFlag::WorkspaceChanged],
            },
        };
        let mut client = client_with_known_threads(vec![known_thread.clone()]);

        let selected = client.use_thread("thread-1".to_string());

        assert_eq!(selected, Some(known_thread));
        client.shutdown();
    }

    #[test]
    fn build_thread_start_params_uses_explicit_interactive_mode() {
        let params = build_thread_start_params(
            AskForApproval::OnRequest,
            Some("gpt-5.4".to_string()),
            Some("openai".to_string()),
            Some("/workspace".to_string()),
        );

        assert_eq!(params.mode, Some(ThreadMode::Interactive));
        assert_eq!(params.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(params.model_provider.as_deref(), Some("openai"));
        assert_eq!(params.cwd.as_deref(), Some("/workspace"));
    }

    #[test]
    fn build_thread_resume_params_leaves_mode_unspecified() {
        let params = build_thread_resume_params(
            "thread-1".to_string(),
            None,
            AskForApproval::OnRequest,
            Some("gpt-5.4".to_string()),
            Some("openai".to_string()),
            Some("/workspace".to_string()),
        );

        assert_eq!(params.mode, None);
        assert_eq!(params.thread_id, "thread-1");
    }

    #[test]
    fn build_thread_resume_params_preserves_explicit_mode() {
        let params = build_thread_resume_params(
            "thread-1".to_string(),
            Some(ThreadMode::ResidentAssistant),
            AskForApproval::OnRequest,
            Some("gpt-5.4".to_string()),
            Some("openai".to_string()),
            Some("/workspace".to_string()),
        );

        assert_eq!(params.mode, Some(ThreadMode::ResidentAssistant));
        assert_eq!(params.thread_id, "thread-1");
    }

    #[test]
    fn denied_permissions_response_returns_empty_grant() {
        assert_eq!(
            denied_permissions_response(&codex_app_server_protocol::RequestPermissionProfile {
                network: Some(codex_app_server_protocol::AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(codex_app_server_protocol::AdditionalFileSystemPermissions {
                    read: Some(vec![]),
                    write: Some(vec![]),
                }),
            }),
            codex_app_server_protocol::GrantedPermissionProfile {
                network: None,
                file_system: None,
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
}
