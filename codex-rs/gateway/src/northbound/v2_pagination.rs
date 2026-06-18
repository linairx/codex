use crate::northbound::v2_connection::DownstreamWorkerHandle;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::request_params;
use codex_app_server_protocol::AppsListParams;
use codex_app_server_protocol::ExperimentalFeatureListParams;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::ListMcpServerStatusParams;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::SortDirection;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadSortKey;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::io;
use std::io::ErrorKind;

pub(crate) const DEFAULT_AGGREGATED_THREAD_LIST_LIMIT: usize = 20;
pub(crate) const AGGREGATED_THREAD_CURSOR_PREFIX: &str = "offset:";
pub(crate) const AGGREGATED_LOADED_THREAD_CURSOR_PREFIX: &str = "loaded-thread-offset:";
pub(crate) const AGGREGATED_APPS_CURSOR_PREFIX: &str = "apps-offset:";
pub(crate) const AGGREGATED_MCP_SERVER_STATUS_CURSOR_PREFIX: &str = "mcp-status-offset:";
pub(crate) const AGGREGATED_MODEL_CURSOR_PREFIX: &str = "model-offset:";
pub(crate) const AGGREGATED_EXPERIMENTAL_FEATURE_CURSOR_PREFIX: &str =
    "experimental-feature-offset:";

/// Request params that can be replayed across worker-local paginated responses
/// while the gateway owns the northbound pagination cursor.
pub(crate) trait CursorPaginatedParams: Serialize + DeserializeOwned {
    fn with_cursor_and_limit(self, cursor: Option<String>, limit: Option<u32>) -> Self;
}

impl CursorPaginatedParams for ThreadListParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

impl CursorPaginatedParams for ThreadLoadedListParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

impl CursorPaginatedParams for AppsListParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

impl CursorPaginatedParams for ListMcpServerStatusParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

impl CursorPaginatedParams for ModelListParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

impl CursorPaginatedParams for ExperimentalFeatureListParams {
    fn with_cursor_and_limit(mut self, cursor: Option<String>, limit: Option<u32>) -> Self {
        self.cursor = cursor;
        self.limit = limit;
        self
    }
}

pub(crate) async fn collect_worker_paginated_data<P, R, T>(
    worker: &DownstreamWorkerHandle,
    request: &JSONRPCRequest,
    mut extract_page: impl FnMut(R) -> (Vec<T>, Option<String>),
) -> io::Result<Vec<T>>
where
    P: CursorPaginatedParams,
    R: DeserializeOwned,
{
    let mut cursor = None;
    let mut items = Vec::new();
    let mut seen_cursors = HashSet::new();

    loop {
        let response: R = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(
                paginated_request_with_overrides::<P>(request, cursor.take(), None)?,
            )?)
            .await
            .map_err(io::Error::other)?;
        let (page_items, next_cursor) = extract_page(response);
        items.extend(page_items);
        let Some(next_cursor) = next_cursor else {
            break;
        };
        if !seen_cursors.insert(next_cursor.clone()) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "downstream {} returned repeated pagination cursor: {next_cursor}",
                    request.method
                ),
            ));
        }
        cursor = Some(next_cursor);
    }

    Ok(items)
}

pub(crate) fn paginated_request_with_overrides<P: CursorPaginatedParams>(
    request: &JSONRPCRequest,
    cursor: Option<String>,
    limit: Option<u32>,
) -> io::Result<JSONRPCRequest> {
    let params = request_params::<P>(request)?;
    Ok(JSONRPCRequest {
        id: request.id.clone(),
        method: request.method.clone(),
        params: Some(
            serde_json::to_value(params.with_cursor_and_limit(cursor, limit))
                .map_err(io::Error::other)?,
        ),
        trace: request.trace.clone(),
    })
}

pub(crate) fn decode_aggregated_offset_cursor(
    cursor: Option<&str>,
    prefix: &str,
    cursor_type: &str,
) -> io::Result<usize> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let Some(offset) = cursor.strip_prefix(prefix) else {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid aggregated {cursor_type} cursor: {cursor}"),
        ));
    };
    offset.parse::<usize>().map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid aggregated {cursor_type} cursor: {cursor}"),
        )
    })
}

pub(crate) fn encode_aggregated_offset_cursor(prefix: &str, offset: usize) -> String {
    format!("{prefix}{offset}")
}

pub(crate) fn aggregated_page_bounds(
    total_len: usize,
    offset: usize,
    limit: usize,
    cursor_prefix: &str,
) -> (usize, usize, Option<String>) {
    let start = offset.min(total_len);
    let end = start.saturating_add(limit).min(total_len);
    let next_cursor =
        (end < total_len).then(|| encode_aggregated_offset_cursor(cursor_prefix, end));
    (start, end, next_cursor)
}

pub(crate) fn sort_threads_for_aggregation(
    threads: &mut [Thread],
    sort_key: Option<ThreadSortKey>,
    sort_direction: Option<SortDirection>,
) {
    match sort_key.unwrap_or(ThreadSortKey::CreatedAt) {
        ThreadSortKey::CreatedAt => {
            if sort_direction.unwrap_or(SortDirection::Desc) == SortDirection::Asc {
                threads.sort_by_key(|thread| (thread.created_at, thread.id.clone()));
            } else {
                threads.sort_by_key(|thread| Reverse((thread.created_at, thread.id.clone())));
            }
        }
        ThreadSortKey::UpdatedAt => {
            if sort_direction.unwrap_or(SortDirection::Desc) == SortDirection::Asc {
                threads.sort_by_key(|thread| (thread.updated_at, thread.id.clone()));
            } else {
                threads.sort_by_key(|thread| Reverse((thread.updated_at, thread.id.clone())));
            }
        }
        ThreadSortKey::RecencyAt => {
            if sort_direction.unwrap_or(SortDirection::Desc) == SortDirection::Asc {
                threads.sort_by_key(|thread| {
                    (
                        thread.recency_at.unwrap_or(thread.updated_at),
                        thread.id.clone(),
                    )
                });
            } else {
                threads.sort_by_key(|thread| {
                    Reverse((
                        thread.recency_at.unwrap_or(thread.updated_at),
                        thread.id.clone(),
                    ))
                });
            }
        }
    }
}
