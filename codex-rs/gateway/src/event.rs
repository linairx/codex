use crate::api::GatewayServerRequest;
use crate::api::GatewayServerRequestEnvelope;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEvent {
    pub method: String,
    pub thread_id: Option<String>,
    pub data: Value,
}

pub(crate) struct GatewayAccountPathHandoffSucceeded<'a> {
    pub tenant_id: &'a str,
    pub project_id: Option<&'a str>,
    pub method: &'a str,
    pub thread_path: &'a str,
    pub exhausted_worker_id: usize,
    pub exhausted_account_id: Option<String>,
    pub replacement_worker_id: usize,
    pub replacement_account_id: Option<String>,
}

pub(crate) struct GatewayAccountPathHandoffFailed<'a> {
    pub tenant_id: &'a str,
    pub project_id: Option<&'a str>,
    pub method: &'a str,
    pub thread_path: &'a str,
    pub exhausted_worker_id: usize,
    pub exhausted_account_id: Option<String>,
    pub reason: &'a str,
}

pub(crate) struct GatewayAccountActiveThreadHandoffFailed<'a> {
    pub tenant_id: &'a str,
    pub project_id: Option<&'a str>,
    pub method: &'a str,
    pub thread_id: &'a str,
    pub exhausted_worker_id: usize,
    pub exhausted_account_id: Option<String>,
    pub reason: &'a str,
}

pub(crate) struct GatewayAccountThreadHandoffSucceeded<'a> {
    pub tenant_id: &'a str,
    pub project_id: Option<&'a str>,
    pub method: &'a str,
    pub thread_id: &'a str,
    pub exhausted_worker_id: usize,
    pub exhausted_account_id: Option<String>,
    pub replacement_worker_id: usize,
    pub replacement_account_id: Option<String>,
}

pub(crate) struct GatewayAccountThreadHandoffFailed<'a> {
    pub tenant_id: &'a str,
    pub project_id: Option<&'a str>,
    pub method: &'a str,
    pub thread_id: &'a str,
    pub exhausted_worker_id: usize,
    pub exhausted_account_id: Option<String>,
    pub reason: &'a str,
}

pub(crate) struct GatewayProjectWorkerRouteSelected<'a> {
    pub tenant_id: &'a str,
    pub project_id: &'a str,
    pub thread_id: &'a str,
    pub worker_id: usize,
    pub account_id: Option<String>,
}

impl GatewayEvent {
    pub fn from_notification(notification: ServerNotification) -> Self {
        let value = serde_json::to_value(notification).unwrap_or(Value::Null);
        Self::from_wire_value(value)
    }

    pub fn rejected_server_request(request: &ServerRequest, reason: &str) -> Self {
        let value = serde_json::to_value(request).unwrap_or(Value::Null);
        let thread_id = value
            .get("params")
            .and_then(extract_thread_id)
            .map(str::to_owned);
        let request_id = match request.id() {
            RequestId::String(id) => Value::String(id.clone()),
            RequestId::Integer(id) => Value::Number((*id).into()),
        };
        let request_method = value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_string();
        Self {
            method: "serverRequest/rejected".to_string(),
            thread_id,
            data: serde_json::json!({
                "requestId": request_id,
                "requestMethod": request_method,
                "reason": reason,
            }),
        }
    }

    pub fn requested_server_request(request_id: RequestId, request: GatewayServerRequest) -> Self {
        let thread_id = Some(request.thread_id().to_string());
        Self {
            method: "serverRequest/requested".to_string(),
            thread_id,
            data: serde_json::to_value(GatewayServerRequestEnvelope {
                request_id,
                request,
            })
            .unwrap_or(Value::Null),
        }
    }

    pub fn lagged(skipped: usize) -> Self {
        Self {
            method: "gateway/lagged".to_string(),
            thread_id: None,
            data: serde_json::json!({ "skipped": skipped }),
        }
    }

    pub fn disconnected(message: String) -> Self {
        Self {
            method: "gateway/disconnected".to_string(),
            thread_id: None,
            data: serde_json::json!({ "message": message }),
        }
    }

    pub fn reconnected(worker_id: usize, websocket_url: String) -> Self {
        Self {
            method: "gateway/reconnected".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "workerId": worker_id,
                "websocketUrl": websocket_url,
            }),
        }
    }

    pub fn reconnecting(
        worker_id: usize,
        websocket_url: String,
        reason: String,
        retry_delay_seconds: u64,
    ) -> Self {
        Self {
            method: "gateway/reconnecting".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "workerId": worker_id,
                "websocketUrl": websocket_url,
                "reason": reason,
                "retryDelaySeconds": retry_delay_seconds,
            }),
        }
    }

    pub fn account_capacity_exhausted(
        tenant_id: &str,
        project_id: Option<&str>,
        worker_id: usize,
        account_id: Option<String>,
        reason: &str,
    ) -> Self {
        Self {
            method: "gateway/accountCapacityExhausted".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "tenantId": tenant_id,
                "projectId": project_id,
                "workerId": worker_id,
                "accountId": account_id,
                "reason": reason,
            }),
        }
    }

    pub fn account_failover_succeeded(
        tenant_id: &str,
        project_id: Option<&str>,
        replacement_worker_id: usize,
        replacement_account_id: Option<String>,
        exhausted_worker_ids: Vec<usize>,
    ) -> Self {
        Self {
            method: "gateway/accountFailoverSucceeded".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "tenantId": tenant_id,
                "projectId": project_id,
                "replacementWorkerId": replacement_worker_id,
                "replacementAccountId": replacement_account_id,
                "exhaustedWorkerIds": exhausted_worker_ids,
            }),
        }
    }

    pub(crate) fn project_worker_route_selected(
        params: GatewayProjectWorkerRouteSelected<'_>,
    ) -> Self {
        Self {
            method: "gateway/projectWorkerRouteSelected".to_string(),
            thread_id: Some(params.thread_id.to_string()),
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "threadId": params.thread_id,
                "workerId": params.worker_id,
                "accountId": params.account_id,
            }),
        }
    }

    pub(crate) fn account_path_handoff_succeeded(
        params: GatewayAccountPathHandoffSucceeded<'_>,
    ) -> Self {
        Self {
            method: "gateway/accountPathHandoffSucceeded".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "method": params.method,
                "threadPath": params.thread_path,
                "exhaustedWorkerId": params.exhausted_worker_id,
                "exhaustedAccountId": params.exhausted_account_id,
                "replacementWorkerId": params.replacement_worker_id,
                "replacementAccountId": params.replacement_account_id,
            }),
        }
    }

    pub(crate) fn account_path_handoff_failed(params: GatewayAccountPathHandoffFailed<'_>) -> Self {
        Self {
            method: "gateway/accountPathHandoffFailed".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "method": params.method,
                "threadPath": params.thread_path,
                "exhaustedWorkerId": params.exhausted_worker_id,
                "exhaustedAccountId": params.exhausted_account_id,
                "reason": params.reason,
            }),
        }
    }

    pub(crate) fn account_active_thread_handoff_failed(
        params: GatewayAccountActiveThreadHandoffFailed<'_>,
    ) -> Self {
        Self {
            method: "gateway/accountActiveThreadHandoffFailed".to_string(),
            thread_id: Some(params.thread_id.to_string()),
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "method": params.method,
                "threadId": params.thread_id,
                "exhaustedWorkerId": params.exhausted_worker_id,
                "exhaustedAccountId": params.exhausted_account_id,
                "reason": params.reason,
            }),
        }
    }

    pub(crate) fn account_thread_handoff_succeeded(
        params: GatewayAccountThreadHandoffSucceeded<'_>,
    ) -> Self {
        Self {
            method: "gateway/accountThreadHandoffSucceeded".to_string(),
            thread_id: Some(params.thread_id.to_string()),
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "method": params.method,
                "threadId": params.thread_id,
                "exhaustedWorkerId": params.exhausted_worker_id,
                "exhaustedAccountId": params.exhausted_account_id,
                "replacementWorkerId": params.replacement_worker_id,
                "replacementAccountId": params.replacement_account_id,
            }),
        }
    }

    pub(crate) fn account_thread_handoff_failed(
        params: GatewayAccountThreadHandoffFailed<'_>,
    ) -> Self {
        Self {
            method: "gateway/accountThreadHandoffFailed".to_string(),
            thread_id: Some(params.thread_id.to_string()),
            data: serde_json::json!({
                "tenantId": params.tenant_id,
                "projectId": params.project_id,
                "method": params.method,
                "threadId": params.thread_id,
                "exhaustedWorkerId": params.exhausted_worker_id,
                "exhaustedAccountId": params.exhausted_account_id,
                "reason": params.reason,
            }),
        }
    }

    fn from_wire_value(value: Value) -> Self {
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_string();
        let data = value.get("params").cloned().unwrap_or(Value::Null);
        let thread_id = extract_thread_id(&data).map(str::to_owned);
        Self {
            method,
            thread_id,
            data,
        }
    }
}

fn extract_thread_id(value: &Value) -> Option<&str> {
    value.get("threadId").and_then(Value::as_str).or_else(|| {
        value
            .get("thread")
            .and_then(|thread| thread.get("id"))
            .and_then(Value::as_str)
    })
}

#[cfg(test)]
mod tests {
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
        let event = GatewayEvent::from_notification(ServerNotification::ThreadStarted(
            ThreadStartedNotification {
                thread: Thread {
                    id: "thread-123".to_string(),
                    forked_from_id: None,
                    preview: "preview".to_string(),
                    ephemeral: true,
                    model_provider: "openai".to_string(),
                    created_at: 1,
                    updated_at: 2,
                    status: codex_app_server_protocol::ThreadStatus::Idle,
                    path: None,
                    cwd: std::path::PathBuf::from("/tmp/project")
                        .try_into()
                        .expect("absolute path"),
                    cli_version: "0.0.0".to_string(),
                    source: SessionSource::Custom("gateway".to_string()),
                    agent_nickname: None,
                    agent_role: None,
                    git_info: None,
                    name: None,
                    turns: Vec::new(),
                },
            },
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
        let event =
            GatewayEvent::project_worker_route_selected(GatewayProjectWorkerRouteSelected {
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
        let event =
            GatewayEvent::account_path_handoff_succeeded(GatewayAccountPathHandoffSucceeded {
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
        let event =
            GatewayEvent::account_thread_handoff_failed(GatewayAccountThreadHandoffFailed {
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
}
