use super::GatewayAccountActiveThreadHandoffFailed;
use super::GatewayAccountPathHandoffFailed;
use super::GatewayAccountPathHandoffSucceeded;
use super::GatewayAccountThreadHandoffFailed;
use super::GatewayAccountThreadHandoffSucceeded;
use super::GatewayEvent;
use super::GatewayProjectWorkerRouteSelected;
use crate::api::GatewayServerRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadStartedNotification;
use codex_app_server_protocol::ToolRequestUserInputOption;
use codex_app_server_protocol::ToolRequestUserInputParams;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn builds_gateway_event_from_notification() {
    let thread = Thread {
        id: "thread-123".to_string(),
        extra: None,
        session_id: "session-123".to_string(),
        forked_from_id: None,
        parent_thread_id: None,
        preview: "preview".to_string(),
        ephemeral: true,
        model_provider: "openai".to_string(),
        created_at: 1,
        updated_at: 2,
        recency_at: Some(2),
        status: codex_app_server_protocol::ThreadStatus::Idle,
        history_mode: Default::default(),
        path: None,
        cwd: std::path::PathBuf::from("/tmp/project")
            .try_into()
            .expect("absolute path"),
        cli_version: "0.0.0".to_string(),
        source: SessionSource::Custom("gateway".to_string()),
        thread_source: None,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: None,
        turns: Vec::new(),
    };
    let event = GatewayEvent::from_notification(ServerNotification::ThreadStarted(
        ThreadStartedNotification { thread },
    ));

    assert_eq!(event.method, "thread/started");
    assert_eq!(event.thread_id, Some("thread-123".to_string()));
}

#[test]
fn reports_rejected_server_requests() {
    let event = GatewayEvent::rejected_server_request(
        &ServerRequest::ToolRequestUserInput {
            request_id: RequestId::String("req-1".to_string()),
            params: ToolRequestUserInputParams {
                auto_resolution_ms: None,
                thread_id: "thread-123".to_string(),
                turn_id: "turn-456".to_string(),
                item_id: "item-789".to_string(),
                questions: vec![ToolRequestUserInputQuestion {
                    id: "confirm".to_string(),
                    header: "Confirm".to_string(),
                    question: "Proceed?".to_string(),
                    is_other: false,
                    is_secret: false,
                    options: Some(vec![ToolRequestUserInputOption {
                        label: "Yes".to_string(),
                        description: "Continue".to_string(),
                    }]),
                }],
            },
        },
        "unsupported",
    );

    assert_eq!(event.method, "serverRequest/rejected");
    assert_eq!(event.thread_id, Some("thread-123".to_string()));
    assert_eq!(
        event.data,
        json!({
            "requestId": "req-1",
            "requestMethod": "item/tool/requestUserInput",
            "reason": "unsupported",
        })
    );
}

#[test]
fn reports_requested_server_requests() {
    let event = GatewayEvent::requested_server_request(
        RequestId::Integer(7),
        GatewayServerRequest::ToolRequestUserInput {
            thread_id: "thread-123".to_string(),
            turn_id: "turn-456".to_string(),
            item_id: "item-789".to_string(),
            auto_resolution_ms: None,
            questions: vec![ToolRequestUserInputQuestion {
                id: "confirm".to_string(),
                header: "Confirm".to_string(),
                question: "Proceed?".to_string(),
                is_other: false,
                is_secret: false,
                options: Some(vec![ToolRequestUserInputOption {
                    label: "Yes".to_string(),
                    description: "Continue".to_string(),
                }]),
            }],
        },
    );

    assert_eq!(event.method, "serverRequest/requested");
    assert_eq!(event.thread_id, Some("thread-123".to_string()));
    assert_eq!(
        event.data,
        json!({
            "requestId": 7,
            "request": {
                "type": "toolRequestUserInput",
                "threadId": "thread-123",
                "turnId": "turn-456",
                "itemId": "item-789",
                "autoResolutionMs": null,
                "questions": [{
                    "id": "confirm",
                    "header": "Confirm",
                    "question": "Proceed?",
                    "isOther": false,
                    "isSecret": false,
                    "options": [{
                        "label": "Yes",
                        "description": "Continue"
                    }]
                }]
            }
        })
    );
}

#[test]
fn reports_reconnected_workers() {
    let event = GatewayEvent::reconnected(2, "ws://127.0.0.1:8082".to_string());

    assert_eq!(event.method, "gateway/reconnected");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "workerId": 2,
            "websocketUrl": "ws://127.0.0.1:8082",
        })
    );
}

#[test]
fn reports_reconnecting_workers() {
    let event = GatewayEvent::reconnecting(
        2,
        "ws://127.0.0.1:8082".to_string(),
        "remote app server event stream ended".to_string(),
        1,
    );

    assert_eq!(event.method, "gateway/reconnecting");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "workerId": 2,
            "websocketUrl": "ws://127.0.0.1:8082",
            "reason": "remote app server event stream ended",
            "retryDelaySeconds": 1,
        })
    );
}

#[test]
fn reports_account_capacity_exhaustion() {
    let event = GatewayEvent::account_capacity_exhausted(
        "tenant-a",
        Some("project-a"),
        1,
        Some("acct-a".to_string()),
        "billing quota reached",
    );

    assert_eq!(event.method, "gateway/accountCapacityExhausted");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 1,
            "accountId": "acct-a",
            "reason": "billing quota reached",
        })
    );
}

#[test]
fn reports_account_lease_changes() {
    let event = GatewayEvent::account_lease_changed(
        "tenant-a",
        Some("project-a"),
        1,
        Some("acct-a".to_string()),
        crate::api::GatewayAccountLeaseState::Cooldown,
        Some("billing quota reached"),
    );

    assert_eq!(event.method, "gateway/accountLeaseChanged");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 1,
            "accountId": "acct-a",
            "leaseState": "cooldown",
            "reason": "billing quota reached",
        })
    );
}

#[test]
fn reports_account_failover_success() {
    let event = GatewayEvent::account_failover_succeeded(
        "tenant-a",
        Some("project-a"),
        2,
        Some("acct-b".to_string()),
        vec![1],
    );

    assert_eq!(event.method, "gateway/accountFailoverSucceeded");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-b",
            "exhaustedWorkerIds": [1],
        })
    );
}

#[test]
fn reports_project_worker_route_selection() {
    let event = GatewayEvent::project_worker_route_selected(GatewayProjectWorkerRouteSelected {
        tenant_id: "tenant-a",
        project_id: "project-a",
        thread_id: "thread-123",
        worker_id: 2,
        account_id: Some("acct-b".to_string()),
    });

    assert_eq!(event.method, "gateway/projectWorkerRouteSelected");
    assert_eq!(event.thread_id.as_deref(), Some("thread-123"));
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "threadId": "thread-123",
            "workerId": 2,
            "accountId": "acct-b",
        })
    );
}

#[test]
fn reports_account_path_handoff_success() {
    let event = GatewayEvent::account_path_handoff_succeeded(GatewayAccountPathHandoffSucceeded {
        tenant_id: "tenant-a",
        project_id: Some("project-a"),
        method: "thread/resume",
        thread_path: "/tmp/shared/rollout.jsonl",
        exhausted_worker_id: 1,
        exhausted_account_id: Some("acct-a".to_string()),
        replacement_worker_id: 2,
        replacement_account_id: Some("acct-b".to_string()),
    });

    assert_eq!(event.method, "gateway/accountPathHandoffSucceeded");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-a",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-b",
        })
    );
}

#[test]
fn reports_account_path_handoff_failure() {
    let event = GatewayEvent::account_path_handoff_failed(GatewayAccountPathHandoffFailed {
        tenant_id: "tenant-a",
        project_id: Some("project-a"),
        method: "getConversationSummary",
        thread_path: "/tmp/shared/rollout.jsonl",
        exhausted_worker_id: 1,
        exhausted_account_id: Some("acct-a".to_string()),
        reason: "no replacement worker restored the context",
    });

    assert_eq!(event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(event.thread_id, None);
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-a",
            "reason": "no replacement worker restored the context",
        })
    );
}

#[test]
fn reports_account_active_thread_handoff_failure() {
    let event = GatewayEvent::account_active_thread_handoff_failed(
        GatewayAccountActiveThreadHandoffFailed {
            tenant_id: "tenant-a",
            project_id: Some("project-a"),
            method: "turn/start",
            thread_id: "thread-1",
            exhausted_worker_id: 1,
            exhausted_account_id: Some("acct-a".to_string()),
            reason: "active thread state cannot be restored without an explicit resume",
        },
    );

    assert_eq!(event.method, "gateway/accountActiveThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "turn/start",
            "threadId": "thread-1",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-a",
            "reason": "active thread state cannot be restored without an explicit resume",
        })
    );
}

#[test]
fn reports_account_thread_handoff_success() {
    let event =
        GatewayEvent::account_thread_handoff_succeeded(GatewayAccountThreadHandoffSucceeded {
            tenant_id: "tenant-a",
            project_id: Some("project-a"),
            method: "thread/resume",
            thread_id: "thread-1",
            exhausted_worker_id: 1,
            exhausted_account_id: Some("acct-a".to_string()),
            replacement_worker_id: 2,
            replacement_account_id: Some("acct-b".to_string()),
        });

    assert_eq!(event.method, "gateway/accountThreadHandoffSucceeded");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-1",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-a",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-b",
        })
    );
}

#[test]
fn reports_account_thread_handoff_failure() {
    let event = GatewayEvent::account_thread_handoff_failed(GatewayAccountThreadHandoffFailed {
        tenant_id: "tenant-a",
        project_id: Some("project-a"),
        method: "thread/resume",
        thread_id: "thread-1",
        exhausted_worker_id: 1,
        exhausted_account_id: Some("acct-a".to_string()),
        reason: "no replacement worker restored the context",
    });

    assert_eq!(event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        event.data,
        json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-1",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-a",
            "reason": "no replacement worker restored the context",
        })
    );
}
