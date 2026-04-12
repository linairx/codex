#![allow(clippy::expect_used)]
use std::io::BufRead;
use std::io::BufReader;
use std::process::ChildStdout;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;

use anyhow::Context;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::all_thread_source_kinds;
use serde::Serialize;
use std::io::Write;

use crate::output::LabelColor;
use crate::output::Output;
use crate::state::KnownThread;
use crate::state::PendingRequest;
use crate::state::ReaderEvent;
use crate::state::State;

#[derive(Clone)]
pub(crate) struct ReaderRequestSink {
    pub(crate) stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
    pub(crate) next_request_id: Arc<AtomicI64>,
}

pub fn start_reader(
    mut stdout: BufReader<ChildStdout>,
    request_sink: ReaderRequestSink,
    state: Arc<Mutex<State>>,
    events: Sender<ReaderEvent>,
    output: Output,
    auto_approve: bool,
    filtered_output: bool,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let command_decision = if auto_approve {
            CommandExecutionApprovalDecision::Accept
        } else {
            CommandExecutionApprovalDecision::Decline
        };
        let file_decision = if auto_approve {
            FileChangeApprovalDecision::Accept
        } else {
            FileChangeApprovalDecision::Decline
        };

        let mut buffer = String::new();

        loop {
            buffer.clear();
            match stdout.read_line(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {}
                Err(err) => {
                    let _ = output.client_line(&format!("failed to read from server: {err}"));
                    break;
                }
            }

            let line = buffer.trim_end_matches(['\n', '\r']);
            if !line.is_empty() && !filtered_output {
                let _ = output.server_line(line);
            }

            let Ok(message) = serde_json::from_str::<JSONRPCMessage>(line) else {
                continue;
            };

            match message {
                JSONRPCMessage::Request(request) => {
                    if let Err(err) = handle_server_request(
                        request,
                        &command_decision,
                        &file_decision,
                        &request_sink.stdin,
                        &output,
                    ) {
                        let _ =
                            output.client_line(&format!("failed to handle server request: {err}"));
                    }
                }
                JSONRPCMessage::Response(response) => {
                    if let Err(err) = handle_response(response, &state, &events) {
                        let _ = output.client_line(&format!("failed to handle response: {err}"));
                    }
                }
                JSONRPCMessage::Notification(notification) => {
                    if let Err(err) = handle_notification(
                        notification,
                        &request_sink,
                        &state,
                        &output,
                        filtered_output,
                    ) {
                        let _ =
                            output.client_line(&format!("failed to handle notification: {err}"));
                    }
                }
                _ => {}
            }
        }
    })
}

fn handle_server_request(
    request: JSONRPCRequest,
    command_decision: &CommandExecutionApprovalDecision,
    file_decision: &FileChangeApprovalDecision,
    stdin: &Arc<Mutex<Option<std::process::ChildStdin>>>,
    output: &Output,
) -> anyhow::Result<()> {
    let server_request = match ServerRequest::try_from(request) {
        Ok(server_request) => server_request,
        Err(_) => return Ok(()),
    };

    match server_request {
        ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
            let response = CommandExecutionRequestApprovalResponse {
                decision: command_decision.clone(),
            };
            output.client_line(&format!(
                "auto-response for command approval {request_id:?}: {command_decision:?} ({params:?})"
            ))?;
            send_response(stdin, request_id, response)
        }
        ServerRequest::FileChangeRequestApproval { request_id, params } => {
            let response = FileChangeRequestApprovalResponse {
                decision: file_decision.clone(),
            };
            output.client_line(&format!(
                "auto-response for file change approval {request_id:?}: {file_decision:?} ({params:?})"
            ))?;
            send_response(stdin, request_id, response)
        }
        _ => Ok(()),
    }
}

fn handle_response(
    response: JSONRPCResponse,
    state: &Arc<Mutex<State>>,
    events: &Sender<ReaderEvent>,
) -> anyhow::Result<()> {
    let pending = {
        let mut state = state.lock().expect("state lock poisoned");
        state.pending.remove(&response.id)
    };

    let Some(pending) = pending else {
        return Ok(());
    };

    match pending {
        PendingRequest::Start => {
            let parsed = serde_json::from_value::<ThreadStartResponse>(response.result)
                .context("decode thread/start response")?;
            let thread_id = parsed.thread.id;
            let thread_name = parsed.thread.name;
            let thread_mode = parsed.thread.mode;
            let thread_status = parsed.thread.status;
            {
                let mut state = state.lock().expect("state lock poisoned");
                state.thread_id = Some(thread_id.clone());
                if let Some(existing) = state
                    .known_threads
                    .iter_mut()
                    .find(|thread| thread.thread_id == thread_id)
                {
                    existing.thread_name = thread_name;
                    existing.thread_mode = thread_mode;
                    existing.thread_status = thread_status;
                } else {
                    state.known_threads.push(KnownThread {
                        thread_id: thread_id.clone(),
                        thread_name,
                        thread_mode,
                        thread_status,
                    });
                }
            }
            events
                .send(ReaderEvent::ThreadReady {
                    thread_id,
                    thread_mode,
                })
                .ok();
        }
        PendingRequest::Resume => {
            let parsed = serde_json::from_value::<ThreadResumeResponse>(response.result)
                .context("decode thread/resume response")?;
            let thread_id = parsed.thread.id;
            let thread_name = parsed.thread.name;
            let thread_mode = parsed.thread.mode;
            let thread_status = parsed.thread.status;
            {
                let mut state = state.lock().expect("state lock poisoned");
                state.thread_id = Some(thread_id.clone());
                if let Some(existing) = state
                    .known_threads
                    .iter_mut()
                    .find(|thread| thread.thread_id == thread_id)
                {
                    existing.thread_name = thread_name;
                    existing.thread_mode = thread_mode;
                    existing.thread_status = thread_status;
                } else {
                    state.known_threads.push(KnownThread {
                        thread_id: thread_id.clone(),
                        thread_name,
                        thread_mode,
                        thread_status,
                    });
                }
            }
            events
                .send(ReaderEvent::ThreadReady {
                    thread_id,
                    thread_mode,
                })
                .ok();
        }
        PendingRequest::List => {
            let parsed = serde_json::from_value::<ThreadListResponse>(response.result)
                .context("decode thread/list response")?;
            let threads: Vec<KnownThread> = parsed
                .data
                .into_iter()
                .map(|thread| KnownThread {
                    thread_id: thread.id,
                    thread_name: thread.name,
                    thread_mode: thread.mode,
                    thread_status: thread.status,
                })
                .collect();
            {
                let mut state = state.lock().expect("state lock poisoned");
                for thread in &threads {
                    if let Some(existing) = state
                        .known_threads
                        .iter_mut()
                        .find(|known| known.thread_id == thread.thread_id)
                    {
                        existing.thread_name = thread.thread_name.clone();
                        existing.thread_mode = thread.thread_mode;
                        existing.thread_status = thread.thread_status.clone();
                    } else {
                        state.known_threads.push(thread.clone());
                    }
                }
            }
            events
                .send(ReaderEvent::ThreadList {
                    threads,
                    next_cursor: parsed.next_cursor,
                })
                .ok();
        }
        PendingRequest::LoadedList => {
            let parsed = serde_json::from_value::<ThreadLoadedListResponse>(response.result)
                .context("decode thread/loaded/list response")?;
            events
                .send(ReaderEvent::LoadedThreadList {
                    thread_ids: parsed.data,
                    next_cursor: parsed.next_cursor,
                })
                .ok();
        }
        PendingRequest::LoadedRead => {
            let parsed = serde_json::from_value::<ThreadLoadedReadResponse>(response.result)
                .context("decode thread/loaded/read response")?;
            let threads: Vec<KnownThread> = parsed
                .data
                .into_iter()
                .map(|thread| KnownThread {
                    thread_id: thread.id,
                    thread_name: thread.name,
                    thread_mode: thread.mode,
                    thread_status: thread.status,
                })
                .collect();
            {
                let mut state = state.lock().expect("state lock poisoned");
                for thread in &threads {
                    if let Some(existing) = state
                        .known_threads
                        .iter_mut()
                        .find(|known| known.thread_id == thread.thread_id)
                    {
                        existing.thread_name = thread.thread_name.clone();
                        existing.thread_mode = thread.thread_mode;
                        existing.thread_status = thread.thread_status.clone();
                    } else {
                        state.known_threads.push(thread.clone());
                    }
                }
            }
            events
                .send(ReaderEvent::LoadedThreadRead {
                    threads,
                    next_cursor: parsed.next_cursor,
                })
                .ok();
        }
    }

    Ok(())
}

fn handle_notification(
    notification: JSONRPCNotification,
    request_sink: &ReaderRequestSink,
    state: &Arc<Mutex<State>>,
    output: &Output,
    filtered_output: bool,
) -> anyhow::Result<()> {
    let Ok(server_notification) = ServerNotification::try_from(notification) else {
        return Ok(());
    };

    match &server_notification {
        ServerNotification::ThreadStarted(payload) => {
            let mut state = state.lock().expect("state lock poisoned");
            let thread = if let Some(existing) = state
                .known_threads
                .iter_mut()
                .find(|thread| thread.thread_id == payload.thread.id)
            {
                existing.thread_name = payload.thread.name.clone();
                existing.thread_mode = payload.thread.mode;
                existing.thread_status = payload.thread.status.clone();
                existing.clone()
            } else {
                let thread = KnownThread {
                    thread_id: payload.thread.id.clone(),
                    thread_name: payload.thread.name.clone(),
                    thread_mode: payload.thread.mode,
                    thread_status: payload.thread.status.clone(),
                };
                state.known_threads.push(thread.clone());
                thread
            };
            drop(state);
            for line in thread_started_summary_lines(&thread) {
                output.client_line(&line)?;
            }
            Ok(())
        }
        ServerNotification::ThreadStatusChanged(payload) => {
            let mut app_state = state.lock().expect("state lock poisoned");
            let known_thread = if let Some(existing) = app_state
                .known_threads
                .iter_mut()
                .find(|thread| thread.thread_id == payload.thread_id)
            {
                existing.thread_status = payload.status.clone();
                Some(existing.clone())
            } else {
                None
            };
            let refresh_request_id = if known_thread.is_none()
                && !app_state
                    .pending
                    .values()
                    .any(|pending| *pending == PendingRequest::LoadedRead)
            {
                let request_id =
                    RequestId::Integer(request_sink.next_request_id.fetch_add(1, Ordering::SeqCst));
                app_state
                    .pending
                    .insert(request_id.clone(), PendingRequest::LoadedRead);
                Some(request_id)
            } else {
                None
            };
            drop(app_state);
            for line in thread_status_changed_summary_lines(
                &payload.thread_id,
                &payload.status,
                known_thread.as_ref(),
            ) {
                output.client_line(&line)?;
            }
            if let Some(request_id) = refresh_request_id
                && let Err(err) = send_client_request(
                    &request_sink.stdin,
                    ClientRequest::ThreadLoadedRead {
                        request_id: request_id.clone(),
                        params: ThreadLoadedReadParams {
                            cursor: None,
                            limit: None,
                            model_providers: None,
                            source_kinds: Some(all_thread_source_kinds()),
                            cwd: None,
                        },
                    },
                )
            {
                let mut app_state = state.lock().expect("state lock poisoned");
                app_state.pending.remove(&request_id);
                return Err(err);
            }
            Ok(())
        }
        ServerNotification::ThreadNameUpdated(payload) => {
            let mut state = state.lock().expect("state lock poisoned");
            let known_thread = if let Some(existing) = state
                .known_threads
                .iter_mut()
                .find(|thread| thread.thread_id == payload.thread_id)
            {
                existing.thread_name = payload.thread_name.clone();
                Some(existing.clone())
            } else {
                None
            };
            drop(state);
            for line in thread_name_updated_summary_lines(
                &payload.thread_id,
                payload.thread_name.as_deref(),
                known_thread.as_ref(),
            ) {
                output.client_line(&line)?;
            }
            Ok(())
        }
        ServerNotification::ItemCompleted(payload) if filtered_output => {
            emit_filtered_item(payload.item.clone(), &payload.thread_id, output)
        }
        _ => Ok(()),
    }
}

fn thread_started_summary_lines(thread: &KnownThread) -> Vec<String> {
    vec![format!("thread started: {}", thread_summary_fields(thread))]
}

fn thread_status_changed_summary_lines(
    thread_id: &str,
    status: &ThreadStatus,
    known_thread: Option<&KnownThread>,
) -> Vec<String> {
    vec![format!(
        "thread status changed: {}",
        thread_status_changed_summary_line(thread_id, status, known_thread)
    )]
}

fn thread_name_updated_summary_lines(
    thread_id: &str,
    thread_name: Option<&str>,
    known_thread: Option<&KnownThread>,
) -> Vec<String> {
    vec![format!(
        "thread name updated: {}",
        thread_name_updated_summary_line(thread_id, thread_name, known_thread)
    )]
}

fn thread_summary_fields(thread: &KnownThread) -> String {
    format!(
        "id={}, name={}, mode={}, status={}, action={}",
        thread.thread_id,
        thread_name_label(thread.thread_name.as_deref()),
        thread_mode_label(thread.thread_mode),
        thread_status_label(&thread.thread_status),
        thread_resume_action_label(thread.thread_mode)
    )
}

fn thread_status_changed_summary_line(
    thread_id: &str,
    status: &ThreadStatus,
    known_thread: Option<&KnownThread>,
) -> String {
    match known_thread {
        Some(thread) => format!(
            "id={thread_id}, name={}, mode={}, status={}, action={}",
            thread_name_label(thread.thread_name.as_deref()),
            thread_mode_label(thread.thread_mode),
            thread_status_label(status),
            thread_resume_action_label(thread.thread_mode)
        ),
        None => format!(
            "id={thread_id}, status={} (refresh thread summary to recover mode/action)",
            thread_status_label(status)
        ),
    }
}

fn thread_name_updated_summary_line(
    thread_id: &str,
    thread_name: Option<&str>,
    known_thread: Option<&KnownThread>,
) -> String {
    match known_thread {
        Some(thread) => format!(
            "id={thread_id}, name={}, mode={}, status={}, action={}",
            thread_name_label(thread_name),
            thread_mode_label(thread.thread_mode),
            thread_status_label(&thread.thread_status),
            thread_resume_action_label(thread.thread_mode)
        ),
        None => format!("id={thread_id}, name={}", thread_name_label(thread_name)),
    }
}

fn thread_name_label(thread_name: Option<&str>) -> &str {
    thread_name.unwrap_or("-")
}

fn thread_mode_label(thread_mode: ThreadMode) -> &'static str {
    match thread_mode {
        ThreadMode::Interactive => "interactive",
        ThreadMode::ResidentAssistant => "resident assistant",
    }
}

fn thread_resume_action_label(thread_mode: ThreadMode) -> &'static str {
    match thread_mode {
        ThreadMode::Interactive => "resume",
        ThreadMode::ResidentAssistant => "reconnect",
    }
}

fn thread_status_label(thread_status: &ThreadStatus) -> String {
    match thread_status {
        ThreadStatus::NotLoaded => "not loaded".to_string(),
        ThreadStatus::Idle => "idle".to_string(),
        ThreadStatus::SystemError => "system error".to_string(),
        ThreadStatus::Active { active_flags } if active_flags.is_empty() => "active".to_string(),
        ThreadStatus::Active { active_flags } => format!(
            "active: {}",
            active_flags
                .iter()
                .copied()
                .map(thread_active_flag_label)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn thread_active_flag_label(
    active_flag: codex_app_server_protocol::ThreadActiveFlag,
) -> &'static str {
    match active_flag {
        codex_app_server_protocol::ThreadActiveFlag::WaitingOnApproval => "waiting on approval",
        codex_app_server_protocol::ThreadActiveFlag::WaitingOnUserInput => "waiting on user input",
        codex_app_server_protocol::ThreadActiveFlag::BackgroundTerminalRunning => {
            "background terminal running"
        }
        codex_app_server_protocol::ThreadActiveFlag::WorkspaceChanged => "workspace changed",
    }
}

fn emit_filtered_item(item: ThreadItem, thread_id: &str, output: &Output) -> anyhow::Result<()> {
    let thread_label = output.format_label(thread_id, LabelColor::Thread);
    match item {
        ThreadItem::AgentMessage { text, .. } => {
            let label = output.format_label("assistant", LabelColor::Assistant);
            output.server_line(&format!("{thread_label} {label}: {text}"))?;
        }
        ThreadItem::Plan { text, .. } => {
            let label = output.format_label("assistant", LabelColor::Assistant);
            output.server_line(&format!("{thread_label} {label}: plan"))?;
            write_multiline(output, &thread_label, &format!("{label}:"), &text)?;
        }
        ThreadItem::CommandExecution {
            command,
            status,
            exit_code,
            aggregated_output,
            ..
        } => {
            let label = output.format_label("tool", LabelColor::Tool);
            output.server_line(&format!(
                "{thread_label} {label}: command {command} ({status:?})"
            ))?;
            if let Some(exit_code) = exit_code {
                let label = output.format_label("tool exit", LabelColor::ToolMeta);
                output.server_line(&format!("{thread_label} {label}: {exit_code}"))?;
            }
            if let Some(aggregated_output) = aggregated_output {
                let label = output.format_label("tool output", LabelColor::ToolMeta);
                write_multiline(
                    output,
                    &thread_label,
                    &format!("{label}:"),
                    &aggregated_output,
                )?;
            }
        }
        ThreadItem::FileChange {
            changes, status, ..
        } => {
            let label = output.format_label("tool", LabelColor::Tool);
            output.server_line(&format!(
                "{thread_label} {label}: file change ({status:?}, {} files)",
                changes.len()
            ))?;
        }
        ThreadItem::McpToolCall {
            server,
            tool,
            status,
            arguments,
            result,
            error,
            ..
        } => {
            let label = output.format_label("tool", LabelColor::Tool);
            output.server_line(&format!(
                "{thread_label} {label}: {server}.{tool} ({status:?})"
            ))?;
            if !arguments.is_null() {
                let label = output.format_label("tool args", LabelColor::ToolMeta);
                output.server_line(&format!("{thread_label} {label}: {arguments}"))?;
            }
            if let Some(result) = result {
                let label = output.format_label("tool result", LabelColor::ToolMeta);
                output.server_line(&format!("{thread_label} {label}: {result:?}"))?;
            }
            if let Some(error) = error {
                let label = output.format_label("tool error", LabelColor::ToolMeta);
                output.server_line(&format!("{thread_label} {label}: {error:?}"))?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn write_multiline(
    output: &Output,
    thread_label: &str,
    header: &str,
    text: &str,
) -> anyhow::Result<()> {
    output.server_line(&format!("{thread_label} {header}"))?;
    for line in text.lines() {
        output.server_line(&format!("{thread_label}   {line}"))?;
    }
    Ok(())
}

fn send_response<T: Serialize>(
    stdin: &Arc<Mutex<Option<std::process::ChildStdin>>>,
    request_id: codex_app_server_protocol::RequestId,
    response: T,
) -> anyhow::Result<()> {
    let result = serde_json::to_value(response).context("serialize response")?;
    let message = JSONRPCResponse {
        id: request_id,
        result,
    };
    let json = serde_json::to_string(&message).context("serialize response message")?;
    let mut line = json;
    line.push('\n');

    let mut stdin = stdin.lock().expect("stdin lock poisoned");
    let Some(stdin) = stdin.as_mut() else {
        anyhow::bail!("stdin already closed");
    };
    stdin.write_all(line.as_bytes()).context("write response")?;
    stdin.flush().context("flush response")?;
    Ok(())
}

fn send_client_request(
    stdin: &Arc<Mutex<Option<std::process::ChildStdin>>>,
    request: ClientRequest,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(&request).context("serialize client request")?;
    let mut line = json;
    line.push('\n');

    let mut stdin = stdin.lock().expect("stdin lock poisoned");
    let Some(stdin) = stdin.as_mut() else {
        anyhow::bail!("stdin already closed");
    };
    stdin
        .write_all(line.as_bytes())
        .context("write client request")?;
    stdin.flush().context("flush client request")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ReaderRequestSink;
    use super::handle_notification;
    use super::handle_response;
    use super::thread_name_updated_summary_line;
    use super::thread_started_summary_lines;
    use super::thread_status_changed_summary_line;
    use crate::output::Output;
    use crate::state::KnownThread;
    use crate::state::PendingRequest;
    use crate::state::ReaderEvent;
    use crate::state::State;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadLoadedReadParams;
    use codex_app_server_protocol::ThreadLoadedReadResponse;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadNameUpdatedNotification;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ThreadStatusChangedNotification;
    use codex_app_server_protocol::all_thread_source_kinds;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicI64;
    use std::sync::mpsc::channel;

    fn notification<T: serde::Serialize>(method: &str, params: T) -> JSONRPCNotification {
        JSONRPCNotification {
            method: method.to_string(),
            params: Some(serde_json::to_value(params).expect("notification params serialize")),
        }
    }

    #[test]
    fn loaded_list_response_does_not_update_known_thread_modes() {
        let request_id = RequestId::Integer(7);
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state
                .pending
                .insert(request_id.clone(), PendingRequest::LoadedList);
            state.known_threads.push(KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            });
        }
        let (events_tx, events_rx) = channel();

        handle_response(
            JSONRPCResponse {
                id: request_id,
                result: serde_json::to_value(ThreadLoadedListResponse {
                    data: vec!["thread-known".to_string(), "thread-unknown".to_string()],
                    next_cursor: Some("cursor-2".to_string()),
                })
                .expect("loaded list response should serialize"),
            },
            &state,
            &events_tx,
        )
        .expect("loaded list response should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            }]
        );
        assert!(state.pending.is_empty());
        drop(state);

        let event = events_rx.recv().expect("loaded list event");
        assert!(matches!(
            event,
            ReaderEvent::LoadedThreadList {
                thread_ids,
                next_cursor,
            } if thread_ids == vec!["thread-known".to_string(), "thread-unknown".to_string()]
                && next_cursor.as_deref() == Some("cursor-2")
        ));
    }

    #[test]
    fn loaded_read_response_updates_known_thread_summaries() {
        let request_id = RequestId::Integer(9);
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state
                .pending
                .insert(request_id.clone(), PendingRequest::LoadedRead);
            state.known_threads.push(KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("old-name".to_string()),
                thread_mode: ThreadMode::Interactive,
                thread_status: ThreadStatus::Idle,
            });
        }
        let (events_tx, events_rx) = channel();

        handle_response(
            JSONRPCResponse {
                id: request_id,
                result: serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![Thread {
                        id: "thread-known".to_string(),
                        forked_from_id: None,
                        preview: "Resident thread".to_string(),
                        ephemeral: false,
                        model_provider: "openai".to_string(),
                        created_at: 1,
                        updated_at: 2,
                        status: ThreadStatus::Active {
                            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                        },
                        mode: ThreadMode::ResidentAssistant,
                        resident: true,
                        path: None,
                        cwd: "/tmp".into(),
                        cli_version: "0.0.0".to_string(),
                        source: SessionSource::Cli,
                        agent_nickname: None,
                        agent_role: None,
                        git_info: None,
                        name: Some("atlas".to_string()),
                        turns: Vec::new(),
                    }],
                    next_cursor: Some("cursor-2".to_string()),
                })
                .expect("loaded read response should serialize"),
            },
            &state,
            &events_tx,
        )
        .expect("loaded read response should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                },
            }]
        );
        assert!(state.pending.is_empty());
        drop(state);

        let event = events_rx.recv().expect("loaded read event");
        assert!(matches!(
            event,
            ReaderEvent::LoadedThreadRead {
                threads,
                next_cursor,
            } if threads == vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                },
            }] && next_cursor.as_deref() == Some("cursor-2")
        ));
    }

    #[test]
    fn thread_started_summary_lines_include_mode_status_and_action() {
        assert_eq!(
            thread_started_summary_lines(&KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            }),
            vec![String::from(
                "thread started: id=thread-known, name=atlas, mode=resident assistant, status=idle, action=reconnect"
            )]
        );
    }

    #[test]
    fn thread_list_response_updates_known_thread_statuses() {
        let request_id = RequestId::Integer(8);
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state
                .pending
                .insert(request_id.clone(), PendingRequest::List);
            state.known_threads.push(KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            });
        }
        let (events_tx, events_rx) = channel();

        handle_response(
            JSONRPCResponse {
                id: request_id,
                result: json!({
                    "data": [{
                        "id": "thread-known",
                        "forkedFromId": null,
                        "preview": "Resident thread",
                        "ephemeral": false,
                        "modelProvider": "openai",
                        "createdAt": 1,
                        "updatedAt": 2,
                        "status": { "type": "systemError" },
                        "mode": "residentAssistant",
                        "resident": true,
                        "path": null,
                        "cwd": "/tmp",
                        "cliVersion": "0.0.0",
                        "source": "cli",
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": "atlas",
                        "turns": []
                    }],
                    "nextCursor": "cursor-2"
                }),
            },
            &state,
            &events_tx,
        )
        .expect("thread list response should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::SystemError,
            }]
        );
        assert!(state.pending.is_empty());
        drop(state);

        let event = events_rx.recv().expect("thread list event");
        assert!(matches!(
            event,
            ReaderEvent::ThreadList {
                threads,
                next_cursor,
            } if threads == vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::SystemError,
            }] && next_cursor.as_deref() == Some("cursor-2")
        ));
    }

    #[test]
    fn thread_started_notification_adds_known_thread_with_mode_and_status() {
        let state = Arc::new(Mutex::new(State::default()));

        handle_notification(
            notification(
                "thread/started",
                ThreadStartedNotification {
                    thread: Thread {
                        id: "thread-known".to_string(),
                        forked_from_id: None,
                        preview: "Resident thread".to_string(),
                        ephemeral: false,
                        model_provider: "openai".to_string(),
                        created_at: 1,
                        updated_at: 2,
                        status: ThreadStatus::Active {
                            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                        },
                        mode: ThreadMode::ResidentAssistant,
                        resident: true,
                        path: None,
                        cwd: "/tmp".into(),
                        cli_version: "0.0.0".to_string(),
                        source: SessionSource::Cli,
                        agent_nickname: None,
                        agent_role: None,
                        git_info: None,
                        name: None,
                        turns: Vec::new(),
                    },
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::new(Mutex::new(None)),
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread started notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                },
            }]
        );
    }

    #[test]
    fn thread_status_changed_notification_updates_known_thread_status() {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state.known_threads.push(KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            });
        }

        handle_notification(
            notification(
                "thread/status/changed",
                ThreadStatusChangedNotification {
                    thread_id: "thread-known".to_string(),
                    status: ThreadStatus::SystemError,
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::new(Mutex::new(None)),
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread status changed notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: None,
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::SystemError,
            }]
        );
    }

    #[test]
    fn thread_status_changed_notification_does_not_infer_unknown_thread_mode() {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state
                .pending
                .insert(RequestId::Integer(99), PendingRequest::LoadedRead);
        }

        handle_notification(
            notification(
                "thread/status/changed",
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::SystemError,
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::new(Mutex::new(None)),
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread status changed notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert!(state.known_threads.is_empty());
    }

    #[test]
    fn unknown_thread_status_change_requests_loaded_read_refresh() {
        let state = Arc::new(Mutex::new(State::default()));
        let next_request_id = Arc::new(AtomicI64::new(11));
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat should start");
        let stdin = Arc::new(Mutex::new(child.stdin.take()));

        handle_notification(
            notification(
                "thread/status/changed",
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::SystemError,
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::clone(&stdin),
                next_request_id: Arc::clone(&next_request_id),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("unknown thread status change should queue loaded/read refresh");

        let stdout = child.stdout.take().expect("cat stdout should exist");
        let mut stdout = BufReader::new(stdout);
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .expect("cat stdout should receive one request");
        let request: JSONRPCRequest =
            serde_json::from_str(line.trim()).expect("request should decode");
        assert_eq!(request.id, RequestId::Integer(11));
        assert_eq!(request.method, "thread/loaded/read");
        assert_eq!(
            request.params,
            Some(
                serde_json::to_value(ThreadLoadedReadParams {
                    cursor: None,
                    limit: None,
                    model_providers: None,
                    source_kinds: Some(all_thread_source_kinds()),
                    cwd: None,
                })
                .expect("loaded read params should serialize")
            )
        );

        let state = state.lock().expect("state lock poisoned");
        assert!(state.known_threads.is_empty());
        assert_eq!(
            state.pending.get(&RequestId::Integer(11)),
            Some(&PendingRequest::LoadedRead)
        );

        drop(state);
        drop(stdin);
        drop(stdout);
        child.wait().expect("cat should exit cleanly");
    }

    #[test]
    fn unknown_thread_status_change_refresh_round_trip_restores_resident_summary() {
        let state = Arc::new(Mutex::new(State::default()));
        let next_request_id = Arc::new(AtomicI64::new(11));
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat should start");
        let stdin = Arc::new(Mutex::new(child.stdin.take()));

        handle_notification(
            notification(
                "thread/status/changed",
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::SystemError,
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::clone(&stdin),
                next_request_id: Arc::clone(&next_request_id),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("unknown thread status change should queue loaded/read refresh");

        let stdout = child.stdout.take().expect("cat stdout should exist");
        let mut stdout = BufReader::new(stdout);
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .expect("cat stdout should receive one request");
        let request: JSONRPCRequest =
            serde_json::from_str(line.trim()).expect("request should decode");
        assert_eq!(request.id, RequestId::Integer(11));

        let (events_tx, _events_rx) = channel();
        handle_response(
            JSONRPCResponse {
                id: RequestId::Integer(11),
                result: serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![Thread {
                        id: "thread-unknown".to_string(),
                        forked_from_id: None,
                        preview: "Resident recovered thread".to_string(),
                        ephemeral: false,
                        model_provider: "openai".to_string(),
                        created_at: 1,
                        updated_at: 2,
                        status: ThreadStatus::SystemError,
                        mode: ThreadMode::ResidentAssistant,
                        resident: true,
                        path: None,
                        cwd: "/tmp".into(),
                        cli_version: "0.0.0".to_string(),
                        source: SessionSource::Cli,
                        agent_nickname: None,
                        agent_role: None,
                        git_info: None,
                        name: Some("atlas".to_string()),
                        turns: Vec::new(),
                    }],
                    next_cursor: None,
                })
                .expect("loaded read response should serialize"),
            },
            &state,
            &events_tx,
        )
        .expect("loaded read response should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-unknown".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::SystemError,
            }]
        );
        assert!(state.pending.is_empty());

        drop(state);
        drop(stdin);
        drop(stdout);
        child.wait().expect("cat should exit cleanly");
    }

    #[test]
    fn thread_status_changed_summary_line_uses_known_mode_and_action_when_available() {
        assert_eq!(
            thread_status_changed_summary_line(
                "thread-known",
                &ThreadStatus::SystemError,
                Some(&KnownThread {
                    thread_id: "thread-known".to_string(),
                    thread_name: Some("atlas".to_string()),
                    thread_mode: ThreadMode::ResidentAssistant,
                    thread_status: ThreadStatus::Idle,
                }),
            ),
            "id=thread-known, name=atlas, mode=resident assistant, status=system error, action=reconnect"
        );
    }

    #[test]
    fn thread_status_changed_summary_line_stays_status_only_for_unknown_threads() {
        assert_eq!(
            thread_status_changed_summary_line("thread-unknown", &ThreadStatus::SystemError, None),
            "id=thread-unknown, status=system error (refresh thread summary to recover mode/action)"
        );
    }

    #[test]
    fn thread_name_updated_notification_refreshes_known_thread_name() {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut state = state.lock().expect("state lock poisoned");
            state.known_threads.push(KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("old-name".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            });
        }

        handle_notification(
            notification(
                "thread/name/updated",
                ThreadNameUpdatedNotification {
                    thread_id: "thread-known".to_string(),
                    thread_name: Some("atlas".to_string()),
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::new(Mutex::new(None)),
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread name updated notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(
            state.known_threads,
            vec![KnownThread {
                thread_id: "thread-known".to_string(),
                thread_name: Some("atlas".to_string()),
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::Idle,
            }]
        );
    }

    #[test]
    fn thread_name_updated_notification_does_not_create_unknown_thread() {
        let state = Arc::new(Mutex::new(State::default()));

        handle_notification(
            notification(
                "thread/name/updated",
                ThreadNameUpdatedNotification {
                    thread_id: "thread-unknown".to_string(),
                    thread_name: Some("atlas".to_string()),
                },
            ),
            &ReaderRequestSink {
                stdin: Arc::new(Mutex::new(None)),
                next_request_id: Arc::new(AtomicI64::new(1)),
            },
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread name updated notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert!(state.known_threads.is_empty());
    }

    #[test]
    fn thread_name_updated_summary_line_uses_known_thread_context_when_available() {
        assert_eq!(
            thread_name_updated_summary_line(
                "thread-known",
                Some("atlas"),
                Some(&KnownThread {
                    thread_id: "thread-known".to_string(),
                    thread_name: Some("old-name".to_string()),
                    thread_mode: ThreadMode::ResidentAssistant,
                    thread_status: ThreadStatus::Idle,
                }),
            ),
            "id=thread-known, name=atlas, mode=resident assistant, status=idle, action=reconnect"
        );
    }

    #[test]
    fn thread_name_updated_summary_line_stays_identity_only_for_unknown_threads() {
        assert_eq!(
            thread_name_updated_summary_line("thread-unknown", Some("atlas"), None),
            "id=thread-unknown, name=atlas"
        );
    }
}
