use crate::config::normalize_remote_account_id;
use crate::error::GatewayError;
use crate::event::GatewayEvent;
use crate::event::GatewayProjectWorkerRouteSelected;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;
use std::io;
use std::io::ErrorKind;
use tracing::info;
use tracing::warn;

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

pub(crate) fn notification_visible_to(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    notification: &JSONRPCNotification,
) -> bool {
    notification
        .params
        .as_ref()
        .and_then(notification_thread_id)
        .is_none_or(|thread_id| scope_registry.thread_visible_to(context, thread_id))
}

pub(crate) fn request_visible_to(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: &JSONRPCRequest,
) -> bool {
    if let Some(thread_path) = request_thread_path(request) {
        return scope_registry.thread_path_visible_to(context, thread_path);
    }
    request_thread_id(request)
        .is_none_or(|thread_id| scope_registry.thread_visible_to(context, thread_id))
}

pub(crate) fn server_request_visible_to(
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    request: &JSONRPCRequest,
) -> bool {
    if request_thread_id(request).is_some() || request_thread_path(request).is_some() {
        return request_visible_to(scope_registry, context, request);
    }

    true
}

pub(crate) fn request_thread_id(request: &JSONRPCRequest) -> Option<&str> {
    if request.method == "thread/resume" && has_non_null_param(request.params.as_ref(), "history") {
        return None;
    }
    if matches!(request.method.as_str(), "thread/resume" | "thread/fork")
        && has_non_null_param(request.params.as_ref(), "path")
    {
        return None;
    }
    request.params.as_ref().and_then(param_thread_id)
}

pub(crate) fn request_thread_path(request: &JSONRPCRequest) -> Option<&str> {
    match request.method.as_str() {
        "thread/resume" | "thread/fork" => request
            .params
            .as_ref()
            .and_then(|params| params.get("path"))
            .and_then(Value::as_str),
        "getConversationSummary" => request
            .params
            .as_ref()
            .and_then(|params| params.get("rolloutPath"))
            .and_then(Value::as_str),
        _ => None,
    }
}

pub(crate) fn response_thread_path(result: &Value) -> Option<&str> {
    result
        .get("thread")
        .and_then(|thread| thread.get("path"))
        .and_then(Value::as_str)
        .or_else(|| {
            result
                .get("summary")
                .and_then(|summary| summary.get("path"))
                .and_then(Value::as_str)
        })
}

fn param_thread_id(params: &Value) -> Option<&str> {
    params
        .get("threadId")
        .and_then(Value::as_str)
        .or_else(|| params.get("conversationId").and_then(Value::as_str))
}

fn has_non_null_param(params: Option<&Value>, name: &str) -> bool {
    params
        .and_then(|params| params.get(name))
        .is_some_and(|value| !value.is_null())
}

pub(crate) fn response_thread_id(result: &Value) -> Option<&str> {
    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .or_else(|| {
            result
                .get("summary")
                .and_then(|summary| summary.get("conversationId"))
                .and_then(Value::as_str)
        })
}

pub(crate) fn notification_thread_id(params: &Value) -> Option<&str> {
    params
        .get("threadId")
        .and_then(Value::as_str)
        .or_else(|| {
            params
                .get("thread")
                .and_then(|thread| thread.get("id"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            params
                .get("turn")
                .and_then(|turn| turn.get("threadId"))
                .and_then(Value::as_str)
        })
}

pub(crate) fn log_recovered_visible_thread_worker_route(
    context: &GatewayRequestContext,
    thread_id: &str,
    worker_id: Option<usize>,
    worker_websocket_url: &str,
) {
    info!(
        tenant_id = context.tenant_id.as_str(),
        project_id = context.project_id.as_deref(),
        thread_id,
        worker_id = ?worker_id,
        worker_websocket_url,
        "recovered missing visible thread route via downstream thread/read probe"
    );
}

pub(crate) fn log_failed_visible_thread_worker_route_recovery(
    context: &GatewayRequestContext,
    thread_id: &str,
    attempted_worker_ids: &[Option<usize>],
    attempted_worker_websocket_urls: &[&str],
) {
    warn!(
        tenant_id = context.tenant_id.as_str(),
        project_id = context.project_id.as_deref(),
        thread_id,
        attempted_worker_ids = ?attempted_worker_ids,
        attempted_worker_websocket_urls = ?attempted_worker_websocket_urls,
        "failed to recover visible thread route via downstream thread/read probe"
    );
}

pub(crate) struct DeduplicatedThreadListEntryLog<'a> {
    pub(crate) thread_id: &'a str,
    pub(crate) selected_worker_id: Option<usize>,
    pub(crate) selected_worker_websocket_url: &'a str,
    pub(crate) discarded_worker_id: Option<usize>,
    pub(crate) discarded_worker_websocket_url: &'a str,
    pub(crate) selected_updated_at: i64,
    pub(crate) discarded_updated_at: i64,
    pub(crate) selected_created_at: i64,
    pub(crate) discarded_created_at: i64,
}

pub(crate) fn log_deduplicated_thread_list_entry(
    request_context: &GatewayRequestContext,
    entry: DeduplicatedThreadListEntryLog<'_>,
) {
    info!(
        tenant_id = request_context.tenant_id.as_str(),
        project_id = request_context.project_id.as_deref(),
        thread_id = entry.thread_id,
        selected_worker_id = ?entry.selected_worker_id,
        selected_worker_websocket_url = entry.selected_worker_websocket_url,
        discarded_worker_id = ?entry.discarded_worker_id,
        discarded_worker_websocket_url = entry.discarded_worker_websocket_url,
        selected_updated_at = entry.selected_updated_at,
        discarded_updated_at = entry.discarded_updated_at,
        selected_created_at = entry.selected_created_at,
        discarded_created_at = entry.discarded_created_at,
        "deduplicating repeated thread/list entry across downstream workers"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::GatewayAccountCapacityStatus;
    use crate::api::GatewayProjectWorkerRoute;
    use crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts;
    use crate::scope::GatewayRequestContext;
    use crate::scope::GatewayScopeRegistry;
    use codex_app_server_protocol::RequestId;
    use opentelemetry_sdk::metrics::data::AggregatedMetrics;
    use opentelemetry_sdk::metrics::data::MetricData;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use tracing_subscriber::layer::SubscriberExt;

    static SYNC_LOG_CAPTURE_LOCK: std::sync::LazyLock<StdMutex<()>> =
        std::sync::LazyLock::new(|| StdMutex::new(()));

    #[derive(Clone, Default)]
    struct SharedWriter {
        buffer: Arc<StdMutex<Vec<u8>>>,
    }

    struct SharedWriterGuard {
        buffer: Arc<StdMutex<Vec<u8>>>,
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
        type Writer = SharedWriterGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedWriterGuard {
                buffer: Arc::clone(&self.buffer),
            }
        }
    }

    impl Write for SharedWriterGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer
                .lock()
                .expect("log buffer should lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_logs(f: impl FnOnce()) -> String {
        let _guard = SYNC_LOG_CAPTURE_LOCK
            .lock()
            .expect("sync log capture lock should lock");
        let writer = SharedWriter::default();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .without_time()
                .with_writer(writer.clone()),
        );
        tracing::subscriber::with_default(subscriber, f);

        let bytes = writer
            .buffer
            .lock()
            .expect("log buffer should lock")
            .clone();
        String::from_utf8(bytes).expect("log output should be utf8")
    }

    fn in_memory_metrics() -> codex_otel::MetricsClient {
        codex_otel::MetricsClient::new(
            codex_otel::MetricsConfig::in_memory(
                "test",
                "codex-gateway",
                env!("CARGO_PKG_VERSION"),
                opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
            )
            .with_runtime_reader(),
        )
        .expect("metrics")
    }

    #[test]
    fn enforce_request_scope_allows_thread_resume_history_bypass() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();

        enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-resume-history".to_string()),
                method: "thread/resume".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "history": [{}],
                })),
                trace: None,
            },
        )
        .expect("history-based resume should be allowed");
    }

    #[test]
    fn enforce_request_scope_allows_thread_resume_visible_path() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_path_with_worker(
            "/tmp/rollout.jsonl",
            context.clone(),
            None,
        );

        enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-resume-path".to_string()),
                method: "thread/resume".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            },
        )
        .expect("path-based resume should be allowed when the path is visible");
    }

    #[test]
    fn enforce_request_scope_allows_thread_fork_visible_path() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_path_with_worker(
            "/tmp/rollout.jsonl",
            context.clone(),
            None,
        );

        enforce_request_scope(
            &scope_registry,
            &context,
            &JSONRPCRequest {
                id: RequestId::String("thread-fork-path".to_string()),
                method: "thread/fork".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-a",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            },
        )
        .expect("path-based fork should be allowed when the path is visible");
    }

    #[test]
    fn request_thread_id_ignores_history_based_thread_resume() {
        let request = JSONRPCRequest {
            id: RequestId::String("thread-resume-history".to_string()),
            method: "thread/resume".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-a",
                "history": [{}],
            })),
            trace: None,
        };

        assert_eq!(request_thread_id(&request), None);
    }

    #[test]
    fn request_thread_id_ignores_path_based_thread_resume_and_fork() {
        for method in ["thread/resume", "thread/fork"] {
            let request = JSONRPCRequest {
                id: RequestId::String(format!("{method}-path")),
                method: method.to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-placeholder",
                    "path": "/tmp/rollout.jsonl",
                })),
                trace: None,
            };

            assert_eq!(request_thread_id(&request), None);
            assert_eq!(request_thread_path(&request), Some("/tmp/rollout.jsonl"));
        }
    }

    #[test]
    fn request_thread_path_uses_rollout_path_for_conversation_summary() {
        let request = JSONRPCRequest {
            id: RequestId::String("conversation-summary-path".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(serde_json::json!({
                "rolloutPath": "/tmp/rollout.jsonl",
            })),
            trace: None,
        };

        assert_eq!(request_thread_id(&request), None);
        assert_eq!(request_thread_path(&request), Some("/tmp/rollout.jsonl"));
    }

    #[test]
    fn response_thread_path_uses_summary_path_for_conversation_summary() {
        let response = serde_json::json!({
            "summary": {
                "conversationId": "thread-a",
                "path": "/tmp/rollout.jsonl",
            },
        });

        assert_eq!(response_thread_path(&response), Some("/tmp/rollout.jsonl"));
    }

    #[test]
    fn enforce_request_scope_rejects_conversation_summary_hidden_rollout_path() {
        let scope_registry = GatewayScopeRegistry::default();
        let visible_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let other_context = GatewayRequestContext {
            tenant_id: "tenant-b".to_string(),
            project_id: Some("project-b".to_string()),
        };
        scope_registry.register_thread_path_with_worker(
            "/tmp/rollout.jsonl",
            visible_context,
            None,
        );

        let err = enforce_request_scope(
            &scope_registry,
            &other_context,
            &JSONRPCRequest {
                id: RequestId::String("conversation-summary-hidden-path".to_string()),
                method: "getConversationSummary".to_string(),
                params: Some(serde_json::json!({
                    "rolloutPath": "/tmp/rollout.jsonl",
                })),
                trace: None,
            },
        )
        .expect_err("hidden rollout summary should be rejected");

        assert!(matches!(
            err,
            GatewayError::NotFound(message) if message == "thread not found: /tmp/rollout.jsonl"
        ));
    }

    #[test]
    fn request_visible_to_uses_conversation_id_for_legacy_exec_command_approval() {
        let scope_registry = GatewayScopeRegistry::default();
        let visible_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let other_context = GatewayRequestContext {
            tenant_id: "tenant-b".to_string(),
            project_id: Some("project-b".to_string()),
        };
        scope_registry.register_thread("thread-visible".to_string(), visible_context.clone());

        let request = JSONRPCRequest {
            id: RequestId::String("legacy-exec-visible".to_string()),
            method: "execCommandApproval".to_string(),
            params: Some(serde_json::json!({
                "conversationId": "thread-visible",
                "callId": "call-visible",
                "approvalId": "approval-visible",
                "command": ["echo", "hello"],
                "cwd": "/tmp/workspace",
                "reason": "Need to run a visible command",
                "parsedCmd": [{
                    "type": "unknown",
                    "cmd": "echo hello",
                }],
            })),
            trace: None,
        };

        assert_eq!(request_thread_id(&request), Some("thread-visible"));
        assert!(request_visible_to(
            &scope_registry,
            &visible_context,
            &request
        ));
        assert!(!request_visible_to(
            &scope_registry,
            &other_context,
            &request
        ));
    }

    #[test]
    fn request_visible_to_uses_conversation_id_for_legacy_apply_patch_approval() {
        let scope_registry = GatewayScopeRegistry::default();
        let visible_context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let other_context = GatewayRequestContext {
            tenant_id: "tenant-b".to_string(),
            project_id: Some("project-b".to_string()),
        };
        scope_registry.register_thread("thread-visible".to_string(), visible_context.clone());

        let request = JSONRPCRequest {
            id: RequestId::String("legacy-patch-visible".to_string()),
            method: "applyPatchApproval".to_string(),
            params: Some(serde_json::json!({
                "conversationId": "thread-visible",
                "callId": "call-visible",
                "fileChanges": {
                    "README.md": {
                        "changeType": "added",
                        "oldContent": null,
                        "newContent": "hello\\n",
                    },
                },
                "reason": "Need to write visible changes",
                "grantRoot": "/tmp/workspace",
            })),
            trace: None,
        };

        assert_eq!(request_thread_id(&request), Some("thread-visible"));
        assert!(request_visible_to(
            &scope_registry,
            &visible_context,
            &request
        ));
        assert!(!request_visible_to(
            &scope_registry,
            &other_context,
            &request
        ));
    }

    #[test]
    fn apply_response_scope_policy_registers_resume_and_fork_threads_and_filters_loaded_list() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-a".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );
        scope_registry.register_thread(
            "thread-hidden".to_string(),
            GatewayRequestContext {
                tenant_id: "tenant-b".to_string(),
                project_id: Some("project-a".to_string()),
            },
        );

        let resume_result = apply_response_scope_policy(
            &scope_registry,
            &context,
            None,
            "thread/resume",
            Some(7),
            None,
            serde_json::json!({
                "thread": {
                    "id": "thread-resumed",
                }
            }),
        )
        .expect("resume response should be accepted");
        assert_eq!(resume_result["thread"]["id"], "thread-resumed");
        assert!(scope_registry.thread_visible_to(&context, "thread-resumed"));

        let fork_result = apply_response_scope_policy(
            &scope_registry,
            &context,
            None,
            "thread/fork",
            Some(8),
            None,
            serde_json::json!({
                "thread": {
                    "id": "thread-forked",
                }
            }),
        )
        .expect("fork response should be accepted");
        assert_eq!(fork_result["thread"]["id"], "thread-forked");
        assert!(scope_registry.thread_visible_to(&context, "thread-forked"));

        let filtered_result = apply_response_scope_policy(
            &scope_registry,
            &context,
            None,
            "thread/loaded/list",
            None,
            None,
            serde_json::json!({
                "data": ["thread-visible", "thread-hidden", "thread-forked"],
            }),
        )
        .expect("loaded list should be filtered");
        assert_eq!(
            filtered_result,
            serde_json::json!({
                "data": ["thread-visible", "thread-forked"],
            })
        );

        let thread_read_result = apply_response_scope_policy(
            &scope_registry,
            &context,
            None,
            "thread/read",
            Some(9),
            None,
            serde_json::json!({
                "thread": {
                    "id": "thread-read",
                }
            }),
        )
        .expect("thread/read response should be accepted");
        assert_eq!(thread_read_result["thread"]["id"], "thread-read");
        assert!(scope_registry.thread_visible_to(&context, "thread-read"));
        assert_eq!(scope_registry.thread_worker_id("thread-read"), Some(9));
    }

    #[test]
    fn apply_response_scope_policy_publishes_project_worker_route_selection_for_thread_start() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(1);
        let observability = GatewayObservability::new(None, true).with_operator_events(event_tx);

        let logs = capture_logs(|| {
            let result = apply_response_scope_policy(
                &scope_registry,
                &context,
                Some(&observability),
                "thread/start",
                Some(7),
                Some("acct-a".to_string()),
                serde_json::json!({
                    "thread": {
                        "id": "thread-started",
                    }
                }),
            )
            .expect("thread/start response should be accepted");

            assert_eq!(result["thread"]["id"], "thread-started");
        });

        assert!(logs.contains("codex_gateway.audit"), "{logs}");
        assert!(
            logs.contains("gateway project worker route selected"),
            "{logs}"
        );
        assert!(logs.contains("worker_id=7"), "{logs}");
        assert!(logs.contains("tenant_id=\"tenant-a\""), "{logs}");
        assert!(logs.contains("project_id=\"project-a\""), "{logs}");
        assert!(logs.contains("thread_id=\"thread-started\""), "{logs}");
        assert!(logs.contains("account_id=\"acct-a\""), "{logs}");

        assert!(scope_registry.thread_visible_to(&context, "thread-started"));
        let event = event_rx.try_recv().expect("project worker route event");
        assert_eq!(event.method, "gateway/projectWorkerRouteSelected");
        assert_eq!(event.thread_id.as_deref(), Some("thread-started"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "threadId": "thread-started",
                "workerId": 7,
                "accountId": "acct-a",
            })
        );
    }

    #[test]
    fn apply_response_scope_policy_publishes_project_worker_route_selection_without_account_id() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(1);
        let metrics = in_memory_metrics();
        let observability =
            GatewayObservability::new(Some(metrics.clone()), true).with_operator_events(event_tx);

        let logs = capture_logs(|| {
            let result = apply_response_scope_policy(
                &scope_registry,
                &context,
                Some(&observability),
                "thread/start",
                Some(7),
                None,
                serde_json::json!({
                    "thread": {
                        "id": "thread-started",
                    }
                }),
            )
            .expect("thread/start response should be accepted");

            assert_eq!(result["thread"]["id"], "thread-started");
        });

        assert!(logs.contains("codex_gateway.audit"), "{logs}");
        assert!(
            logs.contains("gateway project worker route selected"),
            "{logs}"
        );
        assert!(logs.contains("worker_id=7"), "{logs}");
        assert!(logs.contains("tenant_id=\"tenant-a\""), "{logs}");
        assert!(logs.contains("project_id=\"project-a\""), "{logs}");
        assert!(logs.contains("thread_id=\"thread-started\""), "{logs}");
        assert!(logs.contains("account_id=\"<none>\""), "{logs}");

        assert!(scope_registry.thread_visible_to(&context, "thread-started"));
        let event = event_rx.try_recv().expect("project worker route event");
        assert_eq!(event.method, "gateway/projectWorkerRouteSelected");
        assert_eq!(event.thread_id.as_deref(), Some("thread-started"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "threadId": "thread-started",
                "workerId": 7,
                "accountId": null,
            })
        );

        let resource_metrics = metrics.snapshot().expect("snapshot");
        let mut saw_metric = false;
        let mut metric_points = Vec::new();
        for metric in resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        {
            if metric.name() == "gateway_project_worker_route_selections" {
                saw_metric = true;
                match metric.data() {
                    AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                        metric_points.extend(sum.data_points().map(|point| {
                            let attributes = point
                                .attributes()
                                .map(|attribute| {
                                    (
                                        attribute.key.as_str().to_string(),
                                        attribute.value.as_str().to_string(),
                                    )
                                })
                                .collect::<BTreeMap<_, _>>();
                            (point.value(), attributes)
                        }));
                    }
                    _ => panic!("unexpected project worker route metric"),
                }
            }
        }

        metric_points.sort_by(|left, right| left.1.cmp(&right.1));
        assert!(saw_metric);
        assert_eq!(
            metric_points,
            vec![(
                1,
                BTreeMap::from([
                    ("account_id".to_string(), "none".to_string()),
                    ("project_id".to_string(), "project-a".to_string()),
                    ("tenant_id".to_string(), "tenant-a".to_string()),
                    ("worker_id".to_string(), "7".to_string()),
                ])
            )]
        );
    }

    #[test]
    fn apply_response_scope_policy_treats_blank_project_route_account_id_as_missing() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(1);
        let observability = GatewayObservability::new(None, true).with_operator_events(event_tx);

        let logs = capture_logs(|| {
            let result = apply_response_scope_policy(
                &scope_registry,
                &context,
                Some(&observability),
                "thread/start",
                Some(7),
                Some("   ".to_string()),
                serde_json::json!({
                    "thread": {
                        "id": "thread-started",
                    }
                }),
            )
            .expect("thread/start response should be accepted");

            assert_eq!(result["thread"]["id"], "thread-started");
        });

        assert!(logs.contains("codex_gateway.audit"), "{logs}");
        assert!(logs.contains("account_id=\"<none>\""), "{logs}");

        let health = observability.v2_connection_health().snapshot();
        assert_eq!(health.project_worker_route_selection_count, 1);
        assert_eq!(
            health
                .last_project_worker_route_selected_account_id
                .as_deref(),
            None
        );

        let event = event_rx.try_recv().expect("project worker route event");
        assert_eq!(event.method, "gateway/projectWorkerRouteSelected");
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "threadId": "thread-started",
                "workerId": 7,
                "accountId": null,
            })
        );
    }

    #[test]
    fn apply_response_scope_policy_pins_project_worker_route_selection_evidence_chain() {
        let scope_registry = GatewayScopeRegistry::default();
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(1);
        let metrics = in_memory_metrics();
        let observability =
            GatewayObservability::new(Some(metrics.clone()), true).with_operator_events(event_tx);

        let logs = capture_logs(|| {
            let result = apply_response_scope_policy(
                &scope_registry,
                &context,
                Some(&observability),
                "thread/start",
                Some(7),
                Some("acct-a".to_string()),
                serde_json::json!({
                    "thread": {
                        "id": "thread-started",
                    }
                }),
            )
            .expect("thread/start response should be accepted");

            assert_eq!(result["thread"]["id"], "thread-started");
        });

        assert!(logs.contains("codex_gateway.audit"), "{logs}");
        assert!(
            logs.contains("gateway project worker route selected"),
            "{logs}"
        );
        assert!(logs.contains("worker_id=7"), "{logs}");
        assert!(logs.contains("tenant_id=\"tenant-a\""), "{logs}");
        assert!(logs.contains("project_id=\"project-a\""), "{logs}");
        assert!(logs.contains("thread_id=\"thread-started\""), "{logs}");
        assert!(logs.contains("account_id=\"acct-a\""), "{logs}");

        assert!(scope_registry.thread_visible_to(&context, "thread-started"));
        assert_eq!(
            scope_registry.project_worker_routes(
                |_| true,
                |worker_id| match worker_id {
                    7 => Some("acct-a".to_string()),
                    _ => None,
                },
                |_| GatewayAccountCapacityStatus::Available,
            ),
            vec![GatewayProjectWorkerRoute {
                tenant_id: "tenant-a".to_string(),
                project_id: "project-a".to_string(),
                worker_id: 7,
                account_id: Some("acct-a".to_string()),
                account_capacity: GatewayAccountCapacityStatus::Available,
                worker_healthy: true,
                account_routing_eligible: true,
            }]
        );

        let event = event_rx.try_recv().expect("project worker route event");
        assert_eq!(event.method, "gateway/projectWorkerRouteSelected");
        assert_eq!(event.thread_id.as_deref(), Some("thread-started"));
        assert_eq!(
            event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "threadId": "thread-started",
                "workerId": 7,
                "accountId": "acct-a",
            })
        );

        let health = observability.v2_connection_health().snapshot();
        assert_eq!(health.project_worker_route_selection_count, 1);
        assert_eq!(
            health.project_worker_route_selection_worker_counts,
            vec![GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 1,
            }]
        );
        assert_eq!(health.last_project_worker_route_selected_worker_id, Some(7));
        assert_eq!(
            health
                .last_project_worker_route_selected_tenant_id
                .as_deref(),
            Some("tenant-a")
        );
        assert_eq!(
            health
                .last_project_worker_route_selected_project_id
                .as_deref(),
            Some("project-a")
        );
        assert_eq!(
            health
                .last_project_worker_route_selected_thread_id
                .as_deref(),
            Some("thread-started")
        );
        assert_eq!(
            health
                .last_project_worker_route_selected_account_id
                .as_deref(),
            Some("acct-a")
        );
        assert!(health.last_project_worker_route_selected_at.is_some());

        let resource_metrics = metrics.snapshot().expect("snapshot");
        let mut saw_metric = false;
        for metric in resource_metrics
            .scope_metrics()
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        {
            if metric.name() == "gateway_project_worker_route_selections" {
                saw_metric = true;
                match metric.data() {
                    AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                        let point = sum.data_points().next().expect("count point");
                        assert_eq!(point.value(), 1);
                        let attributes: BTreeMap<String, String> = point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str().to_string(),
                                    attribute.value.as_str().to_string(),
                                )
                            })
                            .collect();
                        assert_eq!(
                            attributes,
                            BTreeMap::from([
                                ("account_id".to_string(), "acct-a".to_string()),
                                ("project_id".to_string(), "project-a".to_string()),
                                ("tenant_id".to_string(), "tenant-a".to_string()),
                                ("worker_id".to_string(), "7".to_string()),
                            ])
                        );
                    }
                    _ => panic!("unexpected project worker route metric"),
                }
            }
        }

        assert!(saw_metric);
    }
}
