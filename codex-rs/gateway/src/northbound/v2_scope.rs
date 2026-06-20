use crate::config::normalize_remote_account_id;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use crate::event::GatewayProjectWorkerRouteSelected;
pub(crate) use crate::northbound::v2_scope_logging::DeduplicatedThreadListEntryLog;
pub(crate) use crate::northbound::v2_scope_logging::log_deduplicated_thread_list_entry;
pub(crate) use crate::northbound::v2_scope_logging::log_failed_visible_thread_worker_route_recovery;
pub(crate) use crate::northbound::v2_scope_logging::log_recovered_visible_thread_worker_route;
pub(crate) use crate::northbound::v2_scope_thread::notification_thread_id;
pub(crate) use crate::northbound::v2_scope_thread::notification_visible_to;
pub(crate) use crate::northbound::v2_scope_thread::request_thread_id;
pub(crate) use crate::northbound::v2_scope_thread::request_thread_path;
#[cfg(test)]
pub(crate) use crate::northbound::v2_scope_thread::request_visible_to;
pub(crate) use crate::northbound::v2_scope_thread::response_thread_id;
pub(crate) use crate::northbound::v2_scope_thread::response_thread_path;
pub(crate) use crate::northbound::v2_scope_thread::server_request_visible_to;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;
use std::io;
use std::io::ErrorKind;

pub(crate) fn enforce_request_scope(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: &JSONRPCRequest,
) -> Result<(), GatewayError> {
    match request.method.as_str() {
        "thread/resume" | "thread/fork" | "getConversationSummary" => {
            if let Some(thread_path) = request_thread_path(request)
                && !scope_registry.thread_path_visible_to(context, thread_path)
            {
                return Err(GatewayError::NotFound(format!(
                    "thread not found: {thread_path}"
                )));
            }
        }
        _ => {}
    }

    if let Some(thread_id) = request_thread_id(request)
        && !scope_registry.thread_visible_to(context, thread_id)
    {
        return Err(GatewayError::NotFound(format!(
            "thread not found: {thread_id}"
        )));
    }

    Ok(())
}

pub(crate) fn apply_response_scope_policy(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: Option<&GatewayObservability>,
    method: &str,
    worker_id: Option<usize>,
    worker_account_id: Option<String>,
    mut result: Value,
) -> io::Result<Value> {
    let worker_account_id = normalize_remote_account_id(worker_account_id);

    match method {
        "thread/start"
        | "thread/resume"
        | "thread/fork"
        | "thread/archive"
        | "thread/decrement_elicitation"
        | "thread/increment_elicitation"
        | "thread/inject_items"
        | "thread/memoryMode/set"
        | "thread/metadata/update"
        | "thread/name/set"
        | "thread/read"
        | "thread/rollback"
        | "thread/turns/list"
        | "thread/unarchive" => {
            if let Some(thread_id) = response_thread_id(&result) {
                scope_registry.register_thread_with_worker(
                    thread_id.to_string(),
                    context.clone(),
                    worker_id,
                );
            }
            if method == "thread/start"
                && let Some(worker_id) = worker_id
            {
                scope_registry.register_project_worker(context.clone(), worker_id);
                if let Some(thread_id) = response_thread_id(&result)
                    && let Some(project_id) = context.project_id.as_deref()
                    && let Some(observability) = observability
                {
                    observability.record_project_worker_route_selected(
                        worker_id,
                        context.tenant_id.as_str(),
                        project_id,
                        thread_id,
                        worker_account_id.as_deref(),
                    );
                    observability.publish_operator_event(
                        GatewayEvent::project_worker_route_selected(
                            GatewayProjectWorkerRouteSelected {
                                tenant_id: context.tenant_id.as_str(),
                                project_id,
                                thread_id,
                                worker_id,
                                account_id: worker_account_id,
                            },
                        ),
                    );
                }
            }
            if let Some(thread_path) = response_thread_path(&result) {
                scope_registry.register_thread_path_with_worker(
                    thread_path,
                    context.clone(),
                    worker_id,
                );
            }
        }
        "getConversationSummary" => {
            if let Some(summary) = result.get("summary") {
                let thread_id = summary.get("conversationId").and_then(Value::as_str);
                let thread_path = summary.get("path").and_then(Value::as_str);
                if let Some(thread_id) = thread_id {
                    scope_registry.register_thread_with_worker(
                        thread_id.to_string(),
                        context.clone(),
                        worker_id,
                    );
                }
                if let Some(thread_path) = thread_path {
                    scope_registry.register_thread_path_with_worker(
                        thread_path,
                        context.clone(),
                        worker_id,
                    );
                }
            }
        }
        "review/start" => {
            if let Some(review_thread_id) = result.get("reviewThreadId").and_then(Value::as_str) {
                scope_registry.register_thread_with_worker(
                    review_thread_id.to_string(),
                    context.clone(),
                    worker_id,
                );
            }
        }
        "thread/list" => {
            let data = result
                .get_mut("data")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        "thread/list response is missing data array",
                    )
                })?;
            data.retain(|thread| {
                thread
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|thread_id| {
                        let visible = scope_registry.thread_visible_to(context, thread_id);
                        if visible && let Some(worker_id) = worker_id {
                            scope_registry.register_thread_worker_if_visible(
                                thread_id,
                                context,
                                Some(worker_id),
                            );
                        }
                        if visible
                            && let Some(thread_path) = thread.get("path").and_then(Value::as_str)
                            && let Some(worker_id) = worker_id
                        {
                            scope_registry.register_thread_path_with_worker(
                                thread_path,
                                context.clone(),
                                Some(worker_id),
                            );
                        }
                        visible
                    })
            });
        }
        "thread/loaded/list" => {
            let data = result
                .get_mut("data")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        "thread/loaded/list response is missing data array",
                    )
                })?;
            data.retain(|thread_id| {
                thread_id.as_str().is_some_and(|thread_id| {
                    let visible = scope_registry.thread_visible_to(context, thread_id);
                    if visible && let Some(worker_id) = worker_id {
                        scope_registry.register_thread_worker_if_visible(
                            thread_id,
                            context,
                            Some(worker_id),
                        );
                    }
                    visible
                })
            });
        }
        _ => {}
    }

    Ok(result)
}

#[cfg(test)]
#[path = "v2_scope_tests.rs"]
mod tests;
