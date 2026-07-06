use crate::api::GatewayAccountLeaseState;
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

    pub fn account_lease_changed(
        tenant_id: &str,
        project_id: Option<&str>,
        worker_id: usize,
        account_id: Option<String>,
        lease_state: GatewayAccountLeaseState,
        reason: Option<&str>,
    ) -> Self {
        Self {
            method: "gateway/accountLeaseChanged".to_string(),
            thread_id: None,
            data: serde_json::json!({
                "tenantId": tenant_id,
                "projectId": project_id,
                "workerId": worker_id,
                "accountId": account_id,
                "leaseState": lease_state,
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
#[path = "event_tests.rs"]
mod tests;
