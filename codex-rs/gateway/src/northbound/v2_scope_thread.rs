//! Thread and path scope helpers for northbound v2.
//!
//! This module owns request/response thread extraction and visibility checks so
//! `v2_scope.rs` can focus on scope policy application.

use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;

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
