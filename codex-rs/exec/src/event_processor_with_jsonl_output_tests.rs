use super::*;
use std::path::PathBuf;

use codex_protocol::config_types::ApprovalsReviewer;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::SessionConfiguredEvent;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn failed_turn_does_not_overwrite_output_last_message_file() {
    let tempdir = tempdir().expect("create tempdir");
    let output_path = tempdir.path().join("last-message.txt");
    std::fs::write(&output_path, "keep existing contents").expect("seed output file");

    let mut processor = EventProcessorWithJsonOutput::new(Some(output_path.clone()));

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        codex_app_server_protocol::ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "partial answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(collected.status, CodexStatus::Running);
    assert_eq!(processor.final_message(), Some("partial answer"));

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: codex_app_server_protocol::Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: Some(codex_app_server_protocol::TurnError {
                    message: "turn failed".to_string(),
                    additional_details: None,
                    codex_error_info: None,
                }),
            },
        },
    ));

    assert_eq!(status, CodexStatus::InitiateShutdown);
    assert_eq!(processor.final_message(), None);

    EventProcessor::print_final_output(&mut processor);

    assert_eq!(
        std::fs::read_to_string(&output_path).expect("read output file"),
        "keep existing contents"
    );
}

fn session_configured_event() -> SessionConfiguredEvent {
    SessionConfiguredEvent {
        session_id: codex_protocol::ThreadId::new(),
        forked_from_id: None,
        thread_name: Some("Atlas".to_string()),
        model: "gpt-5.4".to_string(),
        model_provider_id: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        approvals_reviewer: ApprovalsReviewer::default(),
        sandbox_policy: SandboxPolicy::DangerFullAccess,
        cwd: PathBuf::from("/tmp/atlas"),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: None,
    }
}

#[test]
fn thread_started_event_serializes_resident_thread_mode() {
    let event = EventProcessorWithJsonOutput::thread_started_event(
        &session_configured_event(),
        Some(ThreadMode::ResidentAssistant),
    );

    let json = serde_json::to_value(event).expect("serialize thread started event");

    assert_eq!(
        json,
        json!({
            "type": "thread.started",
            "thread_id": json["thread_id"].as_str().expect("thread id"),
            "thread_mode": "residentAssistant"
        })
    );
}

#[test]
fn thread_started_event_omits_thread_mode_when_unknown() {
    let event = EventProcessorWithJsonOutput::thread_started_event(
        &session_configured_event(),
        /*thread_mode*/ None,
    );

    let json = serde_json::to_value(event).expect("serialize thread started event");

    assert_eq!(
        json,
        json!({
            "type": "thread.started",
            "thread_id": json["thread_id"].as_str().expect("thread id")
        })
    );
}
