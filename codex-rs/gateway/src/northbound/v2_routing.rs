//! Routing helpers for northbound v2 worker selection and response validation.
//!
//! This module owns the small routing predicates that decide which downstream
//! worker can handle a request, plus the response-shape checks used by
//! bounded handoff and config selection.

use crate::northbound::v2_connection::DownstreamWorkerHandle;
use crate::northbound::v2_connection::FailClosedMultiWorkerRouteError;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_scope_thread::notification_thread_id;
use crate::northbound::v2_scope_thread::request_thread_id;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;
use std::io;
use std::path::Path;

pub(crate) fn worker_for_notification<'a>(
    downstream: &'a mut GatewayV2DownstreamRouter,
    scope_registry: &crate::scope::GatewayScopeRegistry,
    notification: &JSONRPCNotification,
) -> io::Result<&'a DownstreamWorkerHandle> {
    if !downstream.multi_worker_topology() {
        return downstream.primary_worker();
    }

    if let Some(thread_id) = notification
        .params
        .as_ref()
        .and_then(notification_thread_id)
    {
        if let Ok(worker) = downstream.worker_for_thread(scope_registry, thread_id) {
            return Ok(worker);
        }
        if scope_registry.thread_context(thread_id).is_some() {
            return Err(io::Error::other(format!(
                "thread {thread_id} is missing a downstream worker route"
            )));
        }
    }

    downstream.primary_worker()
}

pub(crate) fn worker_for_request<'a>(
    downstream: &'a mut GatewayV2DownstreamRouter,
    scope_registry: &crate::scope::GatewayScopeRegistry,
    context: &crate::scope::GatewayRequestContext,
    request: &JSONRPCRequest,
) -> io::Result<&'a DownstreamWorkerHandle> {
    if !downstream.multi_worker_topology() {
        return downstream.primary_worker();
    }

    if request.method == "thread/start" {
        return downstream.next_thread_start_worker_for_project(scope_registry, context);
    }

    if requires_primary_worker_route(request) {
        downstream.ensure_primary_worker_present_for(&request.method)?;
        return downstream.primary_worker();
    }

    if let Some(thread_id) = request_thread_id(request) {
        if let Ok(worker) = downstream.worker_for_thread(scope_registry, thread_id) {
            if let Some(worker_id) = worker.worker_id
                && !downstream.worker_account_has_capacity(worker_id)
            {
                return Err(io::Error::other(FailClosedMultiWorkerRouteError {
                    message: format!(
                        "thread {thread_id} is pinned to worker {worker_id} with exhausted account capacity for {}",
                        request.method
                    ),
                }));
            }
            return Ok(worker);
        }
        if scope_registry.thread_context(thread_id).is_some() {
            return Err(io::Error::other(format!(
                "thread {thread_id} is missing a downstream worker route"
            )));
        }
    }

    downstream.primary_worker()
}

pub(crate) fn worker_for_server_request(
    downstream: &GatewayV2DownstreamRouter,
    worker_id: Option<usize>,
) -> io::Result<&DownstreamWorkerHandle> {
    downstream.latest_worker_with_id(worker_id).ok_or_else(|| {
        io::Error::other(format!(
            "gateway v2 connection has no downstream server-request route for worker {worker_id:?}"
        ))
    })
}

pub(crate) fn requires_primary_worker_route(request: &JSONRPCRequest) -> bool {
    matches!(
        request.method.as_str(),
        "configRequirements/read"
            | "account/login/cancel"
            | "account/sendAddCreditsNudgeEmail"
            | "feedback/upload"
            | "command/exec"
            | "command/exec/write"
            | "command/exec/resize"
            | "command/exec/terminate"
            | "fs/readFile"
            | "fs/writeFile"
            | "fs/createDirectory"
            | "fs/getMetadata"
            | "fs/readDirectory"
            | "fs/remove"
            | "fs/copy"
            | "fuzzyFileSearch/sessionStart"
            | "fuzzyFileSearch/sessionUpdate"
            | "fuzzyFileSearch/sessionStop"
            | "windowsSandbox/setupStart"
    ) || (request.method == "account/login/start" && !is_multi_worker_fanout_login_request(request))
}

pub(crate) fn path_handoff_response_must_match_request(request: &JSONRPCRequest) -> bool {
    matches!(
        request.method.as_str(),
        "getConversationSummary" | "thread/resume"
    )
}

pub(crate) fn config_read_response_matches_cwd(result: &Value, cwd: &Path) -> bool {
    result
        .get("layers")
        .and_then(Value::as_array)
        .is_some_and(|layers| {
            layers.iter().any(|layer| {
                layer.get("name").is_some_and(|name| {
                    name.get("dotCodexFolder")
                        .and_then(Value::as_str)
                        .map(Path::new)
                        .is_some_and(|config_dir| cwd.starts_with(config_dir))
                        || name
                            .get("file")
                            .and_then(Value::as_str)
                            .and_then(|file| Path::new(file).parent())
                            .is_some_and(|config_dir| cwd.starts_with(config_dir))
                })
            })
        })
}

fn is_multi_worker_fanout_login_request(request: &JSONRPCRequest) -> bool {
    request.method == "account/login/start"
        && request
            .params
            .as_ref()
            .and_then(|params| params.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|login_type| matches!(login_type, "apiKey" | "chatgptAuthTokens"))
}
