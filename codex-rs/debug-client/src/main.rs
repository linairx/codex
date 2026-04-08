mod client;
mod commands;
mod output;
mod reader;
mod state;

use std::io;
use std::io::BufRead;
use std::sync::mpsc;

use anyhow::Context;
use anyhow::Result;
use clap::ArgAction;
use clap::Parser;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ThreadMode;

use crate::client::AppServerClient;
use crate::client::ThreadConnection;
use crate::client::build_thread_resume_params;
use crate::client::build_thread_start_params;
use crate::commands::InputAction;
use crate::commands::UserCommand;
use crate::commands::parse_input;
use crate::output::Output;
use crate::state::ReaderEvent;

#[derive(Parser)]
#[command(author = "Codex", version, about = "Minimal app-server client")]
struct Cli {
    /// Path to the `codex` CLI binary.
    #[arg(long, default_value = "codex")]
    codex_bin: String,

    /// Forwarded to the `codex` CLI as `--config key=value`. Repeatable.
    #[arg(short = 'c', long = "config", value_name = "key=value", action = ArgAction::Append)]
    config_overrides: Vec<String>,

    /// Resume or reconnect to an existing thread instead of starting a new one.
    #[arg(long)]
    thread_id: Option<String>,

    /// Set the approval policy for the thread.
    #[arg(long, default_value = "on-request")]
    approval_policy: String,

    /// Auto-approve command/file-change approvals.
    #[arg(long, default_value_t = false)]
    auto_approve: bool,

    /// Only show final assistant messages and tool calls.
    #[arg(long, default_value_t = false)]
    final_only: bool,

    /// Optional model override when starting/resuming or reconnecting to a thread.
    #[arg(long)]
    model: Option<String>,

    /// Optional model provider override when starting/resuming or reconnecting to a thread.
    #[arg(long)]
    model_provider: Option<String>,

    /// Optional working directory override when starting/resuming or reconnecting to a thread.
    #[arg(long)]
    cwd: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let output = Output::new();
    let approval_policy = parse_approval_policy(&cli.approval_policy)?;

    let mut client = AppServerClient::spawn(
        &cli.codex_bin,
        &cli.config_overrides,
        output.clone(),
        cli.final_only,
    )?;
    client.initialize()?;

    let thread_connection = if let Some(thread_id) = cli.thread_id.as_ref() {
        client.resume_thread(build_thread_resume_params(
            thread_id.clone(),
            approval_policy,
            cli.model.clone(),
            cli.model_provider.clone(),
            cli.cwd.clone(),
        ))?
    } else {
        client.start_thread(build_thread_start_params(
            approval_policy,
            cli.model.clone(),
            cli.model_provider.clone(),
            cli.cwd.clone(),
        ))?
    };

    output
        .client_line(&format!(
            "{} {}",
            connected_thread_message(&thread_connection),
            thread_connection.thread_id
        ))
        .ok();
    output.set_prompt(&thread_connection.thread_id);

    let (event_tx, event_rx) = mpsc::channel();
    client.start_reader(event_tx, cli.auto_approve, cli.final_only)?;

    print_help(&output);

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        drain_events(&event_rx, &output);
        let prompt_thread = client
            .thread_id()
            .unwrap_or_else(|| "no-thread".to_string());
        output.prompt(&prompt_thread).ok();

        let Some(line) = lines.next() else {
            break;
        };
        let line = line.context("read stdin")?;

        match parse_input(&line) {
            Ok(None) => continue,
            Ok(Some(InputAction::Message(message))) => {
                let Some(active_thread) = client.thread_id() else {
                    output
                        .client_line("no active thread; use :new or :resume <id>")
                        .ok();
                    continue;
                };
                if let Err(err) = client.send_turn(&active_thread, message) {
                    output
                        .client_line(&format!("failed to send turn: {err}"))
                        .ok();
                }
            }
            Ok(Some(InputAction::Command(command))) => {
                if !handle_command(command, &client, &output, approval_policy, &cli) {
                    break;
                }
            }
            Err(err) => {
                output.client_line(&err.message()).ok();
            }
        }
    }

    client.shutdown();
    Ok(())
}

fn handle_command(
    command: UserCommand,
    client: &AppServerClient,
    output: &Output,
    approval_policy: AskForApproval,
    cli: &Cli,
) -> bool {
    match command {
        UserCommand::Help => {
            print_help(output);
            true
        }
        UserCommand::Quit => false,
        UserCommand::NewThread => {
            match client.request_thread_start(build_thread_start_params(
                approval_policy,
                cli.model.clone(),
                cli.model_provider.clone(),
                cli.cwd.clone(),
            )) {
                Ok(request_id) => {
                    output
                        .client_line(&format!("requested new thread ({request_id:?})"))
                        .ok();
                }
                Err(err) => {
                    output
                        .client_line(&format!("failed to start thread: {err}"))
                        .ok();
                }
            }
            true
        }
        UserCommand::Resume(thread_id) => {
            match client.request_thread_resume(build_thread_resume_params(
                thread_id,
                approval_policy,
                cli.model.clone(),
                cli.model_provider.clone(),
                cli.cwd.clone(),
            )) {
                Ok(request_id) => {
                    output
                        .client_line(&format!(
                            "requested thread resume or reconnect ({request_id:?})"
                        ))
                        .ok();
                }
                Err(err) => {
                    output
                        .client_line(&format!("failed to resume or reconnect thread: {err}"))
                        .ok();
                }
            }
            true
        }
        UserCommand::Use(thread_id) => {
            let known_mode = client.use_thread(thread_id.clone());
            output.set_prompt(&thread_id);
            output
                .client_line(&active_thread_switch_message(&thread_id, known_mode))
                .ok();
            true
        }
        UserCommand::RefreshThread => {
            match client.request_thread_list(/*cursor*/ None) {
                Ok(request_id) => {
                    output
                        .client_line(&format!("requested thread list ({request_id:?})"))
                        .ok();
                }
                Err(err) => {
                    output
                        .client_line(&format!("failed to list threads: {err}"))
                        .ok();
                }
            }
            true
        }
    }
}

fn parse_approval_policy(value: &str) -> Result<AskForApproval> {
    match value {
        "untrusted" | "unless-trusted" | "unlessTrusted" => Ok(AskForApproval::UnlessTrusted),
        "on-failure" | "onFailure" => Ok(AskForApproval::OnFailure),
        "on-request" | "onRequest" => Ok(AskForApproval::OnRequest),
        "never" => Ok(AskForApproval::Never),
        _ => anyhow::bail!(
            "unknown approval policy: {value}. Expected one of: untrusted, on-failure, on-request, never"
        ),
    }
}

fn drain_events(event_rx: &mpsc::Receiver<ReaderEvent>, output: &Output) {
    while let Ok(event) = event_rx.try_recv() {
        match event {
            ReaderEvent::ThreadReady {
                thread_id,
                thread_mode,
            } => {
                output
                    .client_line(&format!(
                        "active thread is now {} {thread_id}",
                        thread_ready_label(thread_mode)
                    ))
                    .ok();
                output.set_prompt(&thread_id);
            }
            ReaderEvent::ThreadList {
                threads,
                next_cursor,
            } => {
                if threads.is_empty() {
                    output.client_line("threads: (none)").ok();
                } else {
                    output.client_line("threads:").ok();
                    for thread in threads {
                        output
                            .client_line(&format!(
                                "  {} ({}, {})",
                                thread.thread_id,
                                thread_mode_label(thread.thread_mode),
                                thread_resume_label(thread.thread_mode)
                            ))
                            .ok();
                    }
                }
                if let Some(next_cursor) = next_cursor {
                    output
                        .client_line(&format!(
                            "more threads available, next cursor: {next_cursor}"
                        ))
                        .ok();
                }
            }
        }
    }
}

fn connected_thread_message(thread_connection: &ThreadConnection) -> &'static str {
    match thread_connection.thread_mode {
        ThreadMode::Interactive => "connected to thread",
        ThreadMode::ResidentAssistant => "connected to resident assistant thread",
    }
}

fn active_thread_switch_message(thread_id: &str, thread_mode: Option<ThreadMode>) -> String {
    match thread_mode {
        Some(ThreadMode::Interactive) => format!("switched active thread to thread {thread_id}"),
        Some(ThreadMode::ResidentAssistant) => {
            format!("switched active thread to resident assistant thread {thread_id}")
        }
        None => format!(
            "switched active thread to {thread_id} (unknown; use :resume to load or reconnect)"
        ),
    }
}

fn thread_ready_label(thread_mode: ThreadMode) -> &'static str {
    match thread_mode {
        ThreadMode::Interactive => "thread",
        ThreadMode::ResidentAssistant => "resident assistant thread",
    }
}

fn thread_mode_label(thread_mode: ThreadMode) -> &'static str {
    match thread_mode {
        ThreadMode::Interactive => "interactive",
        ThreadMode::ResidentAssistant => "resident assistant",
    }
}

fn thread_resume_label(thread_mode: ThreadMode) -> &'static str {
    match thread_mode {
        ThreadMode::Interactive => "resume",
        ThreadMode::ResidentAssistant => "reconnect",
    }
}

fn print_help(output: &Output) {
    for line in help_lines() {
        let _ = output.client_line(line);
    }
}

fn help_lines() -> &'static [&'static str] {
    &[
        "commands:",
        "  :help                 show this help",
        "  :new                  start a new thread",
        "  :resume <thread-id>   resume or reconnect to a thread",
        "  :use <thread-id>      switch active thread and preserve known mode",
        "  :refresh-thread       list threads with mode and suggested action",
        "  :quit                 exit",
        "type a message to send it as a new turn",
    ]
}

#[cfg(test)]
mod tests {
    use super::ThreadConnection;
    use super::active_thread_switch_message;
    use super::connected_thread_message;
    use super::help_lines;
    use super::thread_mode_label;
    use super::thread_ready_label;
    use super::thread_resume_label;
    use codex_app_server_protocol::ThreadMode;
    use pretty_assertions::assert_eq;

    #[test]
    fn resident_thread_messages_use_reconnect_language() {
        let thread_connection = ThreadConnection {
            thread_id: "thread-1".to_string(),
            thread_mode: ThreadMode::ResidentAssistant,
        };

        assert_eq!(
            connected_thread_message(&thread_connection),
            "connected to resident assistant thread"
        );
        assert_eq!(
            thread_ready_label(ThreadMode::ResidentAssistant),
            "resident assistant thread"
        );
        assert_eq!(
            thread_resume_label(ThreadMode::ResidentAssistant),
            "reconnect"
        );
    }

    #[test]
    fn interactive_thread_messages_keep_resume_language() {
        let thread_connection = ThreadConnection {
            thread_id: "thread-1".to_string(),
            thread_mode: ThreadMode::Interactive,
        };

        assert_eq!(
            connected_thread_message(&thread_connection),
            "connected to thread"
        );
        assert_eq!(thread_ready_label(ThreadMode::Interactive), "thread");
        assert_eq!(thread_resume_label(ThreadMode::Interactive), "resume");
    }

    #[test]
    fn active_thread_switch_message_uses_thread_mode_when_known() {
        assert_eq!(
            active_thread_switch_message("thread-1", Some(ThreadMode::Interactive)),
            "switched active thread to thread thread-1"
        );
        assert_eq!(
            active_thread_switch_message("thread-1", Some(ThreadMode::ResidentAssistant)),
            "switched active thread to resident assistant thread thread-1"
        );
        assert_eq!(
            active_thread_switch_message("thread-1", None),
            "switched active thread to thread-1 (unknown; use :resume to load or reconnect)"
        );
    }

    #[test]
    fn thread_list_labels_include_mode_and_action() {
        assert_eq!(thread_mode_label(ThreadMode::Interactive), "interactive");
        assert_eq!(
            thread_mode_label(ThreadMode::ResidentAssistant),
            "resident assistant"
        );
        assert_eq!(thread_resume_label(ThreadMode::Interactive), "resume");
        assert_eq!(
            thread_resume_label(ThreadMode::ResidentAssistant),
            "reconnect"
        );
    }

    #[test]
    fn help_text_mentions_mode_aware_thread_commands() {
        let lines = help_lines();
        assert!(
            lines.contains(&"  :use <thread-id>      switch active thread and preserve known mode")
        );
        assert!(
            lines.contains(&"  :refresh-thread       list threads with mode and suggested action")
        );
    }
}
