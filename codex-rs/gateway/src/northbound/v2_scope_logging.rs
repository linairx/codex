//! Logging helpers for northbound v2 scope reconciliation.
//!
//! This module owns the visibility-recovery and thread-deduplication logs so
//! `v2_scope.rs` can stay focused on policy and registration.

use crate::scope::GatewayRequestContext;
use tracing::info;
use tracing::warn;

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
