use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use codex_core::config::ConfigBuilder;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::SessionConfiguredEvent;
use owo_colors::Style;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

use super::EventProcessorWithHumanOutput;
use super::config_summary_entries;
use super::final_message_from_turn_items;
use super::reasoning_text;
use super::should_print_final_message_to_stdout;
use super::should_print_final_message_to_tty;
use crate::event_processor::EventProcessor;

#[test]
fn suppresses_final_stdout_message_when_both_streams_are_terminals() {
    assert!(!should_print_final_message_to_stdout(
        Some("hello"),
        /*stdout_is_terminal*/ true,
        /*stderr_is_terminal*/ true
    ));
}

#[test]
fn prints_final_stdout_message_when_stdout_is_not_terminal() {
    assert!(should_print_final_message_to_stdout(
        Some("hello"),
        /*stdout_is_terminal*/ false,
        /*stderr_is_terminal*/ true
    ));
}

#[test]
fn prints_final_stdout_message_when_stderr_is_not_terminal() {
    assert!(should_print_final_message_to_stdout(
        Some("hello"),
        /*stdout_is_terminal*/ true,
        /*stderr_is_terminal*/ false
    ));
}

#[test]
fn suppresses_final_stdout_message_when_missing() {
    assert!(!should_print_final_message_to_stdout(
        /*final_message*/ None, /*stdout_is_terminal*/ false,
        /*stderr_is_terminal*/ false
    ));
}

#[test]
fn prints_final_tty_message_when_not_yet_rendered() {
    assert!(should_print_final_message_to_tty(
        Some("hello"),
        /*final_message_rendered*/ false,
        /*stdout_is_terminal*/ true,
        /*stderr_is_terminal*/ true
    ));
}

#[test]
fn suppresses_final_tty_message_when_already_rendered() {
    assert!(!should_print_final_message_to_tty(
        Some("hello"),
        /*final_message_rendered*/ true,
        /*stdout_is_terminal*/ true,
        /*stderr_is_terminal*/ true
    ));
}

#[test]
fn reasoning_text_prefers_summary_when_raw_reasoning_is_hidden() {
    let text = reasoning_text(
        &["summary".to_string()],
        &["raw".to_string()],
        /*show_raw_agent_reasoning*/ false,
    );

    assert_eq!(text.as_deref(), Some("summary"));
}

#[test]
fn reasoning_text_uses_raw_content_when_enabled() {
    let text = reasoning_text(
        &["summary".to_string()],
        &["raw".to_string()],
        /*show_raw_agent_reasoning*/ true,
    );

    assert_eq!(text.as_deref(), Some("raw"));
}

#[test]
fn final_message_from_turn_items_uses_latest_agent_message() {
    let message = final_message_from_turn_items(&[
        ThreadItem::AgentMessage {
            id: "msg-1".to_string(),
            text: "first".to_string(),
            phase: None,
            memory_citation: None,
        },
        ThreadItem::Plan {
            id: "plan-1".to_string(),
            text: "plan".to_string(),
        },
        ThreadItem::AgentMessage {
            id: "msg-2".to_string(),
            text: "second".to_string(),
            phase: None,
            memory_citation: None,
        },
    ]);

    assert_eq!(message.as_deref(), Some("second"));
}

#[test]
fn final_message_from_turn_items_falls_back_to_latest_plan() {
    let message = final_message_from_turn_items(&[
        ThreadItem::Reasoning {
            id: "reasoning-1".to_string(),
            summary: vec!["inspect".to_string()],
            content: Vec::new(),
        },
        ThreadItem::Plan {
            id: "plan-1".to_string(),
            text: "first plan".to_string(),
        },
        ThreadItem::Plan {
            id: "plan-2".to_string(),
            text: "final plan".to_string(),
        },
    ]);

    assert_eq!(message.as_deref(), Some("final plan"));
}

#[test]
fn turn_completed_recovers_final_message_from_turn_items() {
    let mut processor = EventProcessorWithHumanOutput {
        bold: Style::new(),
        cyan: Style::new(),
        dimmed: Style::new(),
        green: Style::new(),
        italic: Style::new(),
        magenta: Style::new(),
        red: Style::new(),
        yellow: Style::new(),
        show_agent_reasoning: true,
        show_raw_agent_reasoning: false,
        last_message_path: None,
        final_message: None,
        final_message_rendered: false,
        emit_final_message_on_shutdown: false,
        last_total_token_usage: None,
    };

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        status,
        crate::event_processor::CodexStatus::InitiateShutdown
    );
    assert_eq!(processor.final_message.as_deref(), Some("final answer"));
}

#[test]
fn turn_completed_overwrites_stale_final_message_from_turn_items() {
    let mut processor = EventProcessorWithHumanOutput {
        bold: Style::new(),
        cyan: Style::new(),
        dimmed: Style::new(),
        green: Style::new(),
        italic: Style::new(),
        magenta: Style::new(),
        red: Style::new(),
        yellow: Style::new(),
        show_agent_reasoning: true,
        show_raw_agent_reasoning: false,
        last_message_path: None,
        final_message: Some("stale answer".to_string()),
        final_message_rendered: true,
        emit_final_message_on_shutdown: false,
        last_total_token_usage: None,
    };

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        status,
        crate::event_processor::CodexStatus::InitiateShutdown
    );
    assert_eq!(processor.final_message.as_deref(), Some("final answer"));
    assert!(!processor.final_message_rendered);
}

#[test]
fn turn_completed_preserves_streamed_final_message_when_turn_items_are_empty() {
    let mut processor = EventProcessorWithHumanOutput {
        bold: Style::new(),
        cyan: Style::new(),
        dimmed: Style::new(),
        green: Style::new(),
        italic: Style::new(),
        magenta: Style::new(),
        red: Style::new(),
        yellow: Style::new(),
        show_agent_reasoning: true,
        show_raw_agent_reasoning: false,
        last_message_path: None,
        final_message: Some("streamed answer".to_string()),
        final_message_rendered: false,
        emit_final_message_on_shutdown: false,
        last_total_token_usage: None,
    };

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        status,
        crate::event_processor::CodexStatus::InitiateShutdown
    );
    assert_eq!(processor.final_message.as_deref(), Some("streamed answer"));
    assert!(processor.emit_final_message_on_shutdown);
}

#[test]
fn turn_failed_clears_stale_final_message() {
    let mut processor = EventProcessorWithHumanOutput {
        bold: Style::new(),
        cyan: Style::new(),
        dimmed: Style::new(),
        green: Style::new(),
        italic: Style::new(),
        magenta: Style::new(),
        red: Style::new(),
        yellow: Style::new(),
        show_agent_reasoning: true,
        show_raw_agent_reasoning: false,
        last_message_path: None,
        final_message: Some("partial answer".to_string()),
        final_message_rendered: true,
        emit_final_message_on_shutdown: true,
        last_total_token_usage: None,
    };

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: None,
            },
        },
    ));

    assert_eq!(
        status,
        crate::event_processor::CodexStatus::InitiateShutdown
    );
    assert_eq!(processor.final_message, None);
    assert!(!processor.final_message_rendered);
    assert!(!processor.emit_final_message_on_shutdown);
}

#[test]
fn turn_interrupted_clears_stale_final_message() {
    let mut processor = EventProcessorWithHumanOutput {
        bold: Style::new(),
        cyan: Style::new(),
        dimmed: Style::new(),
        green: Style::new(),
        italic: Style::new(),
        magenta: Style::new(),
        red: Style::new(),
        yellow: Style::new(),
        show_agent_reasoning: true,
        show_raw_agent_reasoning: false,
        last_message_path: None,
        final_message: Some("partial answer".to_string()),
        final_message_rendered: true,
        emit_final_message_on_shutdown: true,
        last_total_token_usage: None,
    };

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Interrupted,
                error: None,
            },
        },
    ));

    assert_eq!(
        status,
        crate::event_processor::CodexStatus::InitiateShutdown
    );
    assert_eq!(processor.final_message, None);
    assert!(!processor.final_message_rendered);
    assert!(!processor.emit_final_message_on_shutdown);
}

#[tokio::test]
async fn config_summary_entries_include_resident_session_mode() {
    let codex_home = tempdir().expect("create codex home");
    let cwd = tempdir().expect("create cwd");
    let config = ConfigBuilder::default()
        .codex_home(codex_home.path().to_path_buf())
        .fallback_cwd(Some(cwd.path().to_path_buf()))
        .build()
        .await
        .expect("build config");
    let session_configured = SessionConfiguredEvent {
        session_id: codex_protocol::ThreadId::new(),
        forked_from_id: None,
        thread_name: Some("Atlas".to_string()),
        model: "gpt-5.4".to_string(),
        model_provider_id: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        approvals_reviewer: Default::default(),
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        cwd: cwd.path().to_path_buf(),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: None,
    };

    let entries = config_summary_entries(
        &config,
        &session_configured,
        Some(ThreadMode::ResidentAssistant),
    );

    assert!(entries.contains(&("session mode", "resident assistant".to_string())));
    assert!(entries.contains(&("session action", "reconnect".to_string())));
}

#[tokio::test]
async fn config_summary_entries_include_interactive_session_action() {
    let codex_home = tempdir().expect("create codex home");
    let cwd = tempdir().expect("create cwd");
    let config = ConfigBuilder::default()
        .codex_home(codex_home.path().to_path_buf())
        .fallback_cwd(Some(cwd.path().to_path_buf()))
        .build()
        .await
        .expect("build config");
    let session_configured = SessionConfiguredEvent {
        session_id: codex_protocol::ThreadId::new(),
        forked_from_id: None,
        thread_name: Some("Atlas".to_string()),
        model: "gpt-5.4".to_string(),
        model_provider_id: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        approvals_reviewer: Default::default(),
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        cwd: cwd.path().to_path_buf(),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: None,
    };

    let entries =
        config_summary_entries(&config, &session_configured, Some(ThreadMode::Interactive));

    assert!(entries.contains(&("session mode", "interactive".to_string())));
    assert!(entries.contains(&("session action", "resume".to_string())));
}

#[tokio::test]
async fn config_summary_entries_omit_session_mode_and_action_when_unknown() {
    let codex_home = tempdir().expect("create codex home");
    let cwd = tempdir().expect("create cwd");
    let config = ConfigBuilder::default()
        .codex_home(codex_home.path().to_path_buf())
        .fallback_cwd(Some(cwd.path().to_path_buf()))
        .build()
        .await
        .expect("build config");
    let session_configured = SessionConfiguredEvent {
        session_id: codex_protocol::ThreadId::new(),
        forked_from_id: None,
        thread_name: Some("Atlas".to_string()),
        model: "gpt-5.4".to_string(),
        model_provider_id: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        approvals_reviewer: Default::default(),
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        cwd: cwd.path().to_path_buf(),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: None,
    };

    let entries = config_summary_entries(&config, &session_configured, /*thread_mode*/ None);

    assert!(!entries.iter().any(|(key, _)| *key == "session mode"));
    assert!(!entries.iter().any(|(key, _)| *key == "session action"));
}
