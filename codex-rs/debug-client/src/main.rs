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
                    output.client_line(no_active_thread_message()).ok();
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
        UserCommand::RefreshThread(cursor) => {
            match client.request_thread_list(cursor.clone()) {
                Ok(request_id) => {
                    output
                        .client_line(&match cursor {
                            Some(cursor) => {
                                format!("requested thread list ({request_id:?}, cursor={cursor})")
                            }
                            None => format!("requested thread list ({request_id:?})"),
                        })
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
                for line in thread_list_lines(&threads, next_cursor.as_deref()) {
                    output.client_line(&line).ok();
                }
            }
        }
    }
}

fn thread_list_lines(
    threads: &[crate::state::KnownThread],
    next_cursor: Option<&str>,
) -> Vec<String> {
    let mut lines = if threads.is_empty() {
        vec!["threads: (none)".to_string()]
    } else {
        let mut lines = Vec::with_capacity(threads.len() + 1);
        lines.push("threads:".to_string());
        lines.extend(threads.iter().map(|thread| {
            format!(
                "  {} ({}, {})",
                thread.thread_id,
                thread_mode_label(thread.thread_mode),
                thread_resume_label(thread.thread_mode)
            )
        }));
        lines
    };

    if let Some(next_cursor) = next_cursor {
        lines.push(format!(
            "more threads available, next cursor: {next_cursor}"
        ));
    }

    lines
}

fn connected_thread_message(thread_connection: &ThreadConnection) -> &'static str {
    match thread_connection.thread_mode {
        ThreadMode::Interactive => "connected to thread",
        ThreadMode::ResidentAssistant => "connected to resident assistant thread",
    }
}

fn no_active_thread_message() -> &'static str {
    "no active thread; use :new or :resume <id> to resume or reconnect"
}

fn active_thread_switch_message(thread_id: &str, thread_mode: Option<ThreadMode>) -> String {
    match thread_mode {
        Some(ThreadMode::Interactive) => format!("switched active thread to thread {thread_id}"),
        Some(ThreadMode::ResidentAssistant) => {
            format!("switched active thread to resident assistant thread {thread_id}")
        }
        None => format!(
            "switched active thread to {thread_id} (unknown; use :resume to resume or reconnect)"
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
        "  :use <thread-id>      switch active thread without resuming/reconnecting",
        "  :refresh-thread [cursor] list threads with mode, suggested action, and optional pagination cursor",
        "  :quit                 exit",
        "type a message to send it as a new turn",
    ]
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use super::ThreadConnection;
    use super::active_thread_switch_message;
    use super::connected_thread_message;
    use super::help_lines;
    use super::no_active_thread_message;
    use super::thread_list_lines;
    use super::thread_mode_label;
    use super::thread_ready_label;
    use super::thread_resume_label;
    use crate::state::KnownThread;
    use clap::CommandFactory;
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
            "switched active thread to thread-1 (unknown; use :resume to resume or reconnect)"
        );
    }

    #[test]
    fn no_active_thread_message_mentions_resume_or_reconnect() {
        assert_eq!(
            no_active_thread_message(),
            "no active thread; use :new or :resume <id> to resume or reconnect"
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
        assert!(lines.contains(
            &"  :use <thread-id>      switch active thread without resuming/reconnecting"
        ));
        assert!(
            lines.contains(
                &"  :refresh-thread [cursor] list threads with mode, suggested action, and optional pagination cursor"
            )
        );
    }

    #[test]
    fn clap_help_mentions_resume_or_reconnect() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("Resume or reconnect to an existing thread"));
        assert!(help.contains("starting/resuming or reconnecting to a thread"));
    }

    #[test]
    fn thread_list_lines_render_mode_and_action_labels() {
        let lines = thread_list_lines(
            &[
                KnownThread {
                    thread_id: "thread-1".to_string(),
                    thread_mode: ThreadMode::Interactive,
                },
                KnownThread {
                    thread_id: "thread-2".to_string(),
                    thread_mode: ThreadMode::ResidentAssistant,
                },
            ],
            Some("cursor-2"),
        );

        assert_eq!(
            lines,
            vec![
                "threads:".to_string(),
                "  thread-1 (interactive, resume)".to_string(),
                "  thread-2 (resident assistant, reconnect)".to_string(),
                "more threads available, next cursor: cursor-2".to_string(),
            ]
        );
    }

    #[test]
    fn thread_list_lines_render_empty_state_without_cursor() {
        let lines = thread_list_lines(&[], None);

        assert_eq!(lines, vec!["threads: (none)".to_string()]);
    }

    #[test]
    fn refresh_thread_request_message_mentions_cursor_when_present() {
        let request_id = codex_app_server_protocol::RequestId::Integer(7);
        let cursor = "cursor-2".to_string();

        let message = match Some(cursor) {
            Some(cursor) => format!("requested thread list ({request_id:?}, cursor={cursor})"),
            None => format!("requested thread list ({request_id:?})"),
        };

        assert_eq!(
            message,
            "requested thread list (Integer(7), cursor=cursor-2)"
        );
    }
}
