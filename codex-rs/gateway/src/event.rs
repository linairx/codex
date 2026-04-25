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
    use super::GatewayEvent;
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
}
