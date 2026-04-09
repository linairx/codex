#![allow(clippy::expect_used)]
use std::io::BufRead;
use std::io::BufReader;
use std::process::ChildStdout;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;

use anyhow::Context;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartResponse;
use serde::Serialize;
use std::io::Write;

use crate::output::LabelColor;
use crate::output::Output;
use crate::state::KnownThread;
use crate::state::PendingRequest;
use crate::state::ReaderEvent;
use crate::state::State;

pub fn start_reader(
    mut stdout: BufReader<ChildStdout>,
    stdin: Arc<Mutex<Option<std::process::ChildStdin>>>,
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
                        &stdin,
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
                    if let Err(err) =
                        handle_notification(notification, &state, &output, filtered_output)
                    {
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
                    existing.thread_mode = thread_mode;
                    existing.thread_status = thread_status;
                } else {
                    state.known_threads.push(KnownThread {
                        thread_id: thread_id.clone(),
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
                    existing.thread_mode = thread_mode;
                    existing.thread_status = thread_status;
                } else {
                    state.known_threads.push(KnownThread {
                        thread_id: thread_id.clone(),
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
    }

    Ok(())
}

fn handle_notification(
    notification: JSONRPCNotification,
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
            if let Some(existing) = state
                .known_threads
                .iter_mut()
                .find(|thread| thread.thread_id == payload.thread.id)
            {
                existing.thread_mode = payload.thread.mode;
                existing.thread_status = payload.thread.status.clone();
            } else {
                state.known_threads.push(KnownThread {
                    thread_id: payload.thread.id.clone(),
                    thread_mode: payload.thread.mode,
                    thread_status: payload.thread.status.clone(),
                });
            }
            Ok(())
        }
        ServerNotification::ThreadStatusChanged(payload) => {
            let mut state = state.lock().expect("state lock poisoned");
            if let Some(existing) = state
                .known_threads
                .iter_mut()
                .find(|thread| thread.thread_id == payload.thread_id)
            {
                existing.thread_status = payload.status.clone();
            }
            Ok(())
        }
        ServerNotification::ItemCompleted(payload) if filtered_output => {
            emit_filtered_item(payload.item.clone(), &payload.thread_id, output)
        }
        _ => Ok(()),
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

#[cfg(test)]
mod tests {
    use super::handle_notification;
    use super::handle_response;
    use crate::output::Output;
    use crate::state::KnownThread;
    use crate::state::PendingRequest;
    use crate::state::ReaderEvent;
    use crate::state::State;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ThreadStatusChangedNotification;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex;
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
                        "name": null,
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
                thread_mode: ThreadMode::ResidentAssistant,
                thread_status: ThreadStatus::SystemError,
            }]
        );
    }

    #[test]
    fn thread_status_changed_notification_does_not_infer_unknown_thread_mode() {
        let state = Arc::new(Mutex::new(State::default()));

        handle_notification(
            notification(
                "thread/status/changed",
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::SystemError,
                },
            ),
            &state,
            &Output::new(),
            /*filtered_output*/ false,
        )
        .expect("thread status changed notification should decode");

        let state = state.lock().expect("state lock poisoned");
        assert!(state.known_threads.is_empty());
    }
}
