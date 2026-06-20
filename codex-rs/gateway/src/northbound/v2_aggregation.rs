//! Response aggregation helpers for northbound v2 multi-worker fan-out.
//!
//! This module owns the cross-worker collection and merge logic for the thread,
//! app, and account aggregation paths. Catalog-style aggregation lives in
//! `v2_aggregation_catalog.rs`.

use crate::northbound::v2_account_capacity::sync_worker_account_capacity_from_rate_limits_response;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection_runtime::log_degraded_multi_worker_thread_discovery;
use crate::northbound::v2_pagination::AGGREGATED_APPS_CURSOR_PREFIX;
use crate::northbound::v2_pagination::AGGREGATED_LOADED_THREAD_CURSOR_PREFIX;
use crate::northbound::v2_pagination::AGGREGATED_MCP_SERVER_STATUS_CURSOR_PREFIX;
use crate::northbound::v2_pagination::AGGREGATED_THREAD_CURSOR_PREFIX;
use crate::northbound::v2_pagination::DEFAULT_AGGREGATED_THREAD_LIST_LIMIT;
use crate::northbound::v2_pagination::aggregated_page_bounds;
use crate::northbound::v2_pagination::collect_worker_paginated_data;
use crate::northbound::v2_pagination::decode_aggregated_offset_cursor;
use crate::northbound::v2_pagination::encode_aggregated_offset_cursor;
use crate::northbound::v2_pagination::sort_threads_for_aggregation;
use crate::northbound::v2_scope::DeduplicatedThreadListEntryLog;
use crate::northbound::v2_scope::log_deduplicated_thread_list_entry;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::request_params;
use crate::observability::GatewayObservability;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use codex_app_server_protocol::AppInfo;
use codex_app_server_protocol::AppsListParams;
use codex_app_server_protocol::AppsListResponse;
use codex_app_server_protocol::ExternalAgentConfigDetectResponse;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::GetAuthStatusResponse;
use codex_app_server_protocol::ListMcpServerStatusParams;
use codex_app_server_protocol::ListMcpServerStatusResponse;
use codex_app_server_protocol::McpServerStatus;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io;

pub(crate) use crate::northbound::v2_aggregation_catalog::*;

pub(crate) async fn aggregate_thread_list_response(
    downstream: &GatewayV2DownstreamRouter,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    log_degraded_multi_worker_thread_discovery(downstream, context, observability, &request.method);

    let params = request_params::<ThreadListParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_THREAD_CURSOR_PREFIX,
        "thread",
    )?;
    let limit = params
        .limit
        .unwrap_or(DEFAULT_AGGREGATED_THREAD_LIST_LIMIT as u32) as usize;
    let mut threads_by_id =
        HashMap::<String, (codex_app_server_protocol::Thread, Option<usize>)>::new();

    for worker in &downstream.workers {
        let pages = collect_worker_paginated_data::<ThreadListParams, ThreadListResponse, _>(
            worker,
            request,
            |response| (response.data, response.next_cursor),
        )
        .await?;
        for thread in pages.into_iter().filter(|thread| {
            if !scope_registry.thread_visible_to(context, &thread.id) {
                return false;
            }
            true
        }) {
            match threads_by_id.entry(thread.id.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert((thread, worker.worker_id));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let (
                        replace_existing,
                        selected_worker_id,
                        discarded_worker_id,
                        selected_updated_at,
                        discarded_updated_at,
                        selected_created_at,
                        discarded_created_at,
                    ) = {
                        let (existing, existing_worker_id) = entry.get();
                        if (thread.updated_at, thread.created_at)
                            > (existing.updated_at, existing.created_at)
                        {
                            (
                                true,
                                worker.worker_id,
                                *existing_worker_id,
                                thread.updated_at,
                                existing.updated_at,
                                thread.created_at,
                                existing.created_at,
                            )
                        } else {
                            (
                                false,
                                *existing_worker_id,
                                worker.worker_id,
                                existing.updated_at,
                                thread.updated_at,
                                existing.created_at,
                                thread.created_at,
                            )
                        }
                    };
                    log_deduplicated_thread_list_entry(
                        context,
                        DeduplicatedThreadListEntryLog {
                            thread_id: thread.id.as_str(),
                            selected_worker_id,
                            selected_worker_websocket_url: downstream
                                .websocket_url_for_worker_id(selected_worker_id),
                            discarded_worker_id,
                            discarded_worker_websocket_url: downstream
                                .websocket_url_for_worker_id(discarded_worker_id),
                            selected_updated_at,
                            discarded_updated_at,
                            selected_created_at,
                            discarded_created_at,
                        },
                    );
                    observability.record_v2_thread_list_deduplication(selected_worker_id);
                    if replace_existing {
                        entry.insert((thread, worker.worker_id));
                    }
                }
            }
        }
    }

    let mut threads = threads_by_id
        .into_values()
        .map(|(thread, worker_id)| {
            scope_registry.register_thread_worker_if_visible(&thread.id, context, worker_id);
            thread
        })
        .collect::<Vec<_>>();

    sort_threads_for_aggregation(&mut threads, params.sort_key, params.sort_direction);
    let page: Vec<_> = threads.iter().skip(offset).take(limit).cloned().collect();
    let next_cursor = (offset + page.len() < threads.len()).then(|| {
        encode_aggregated_offset_cursor(AGGREGATED_THREAD_CURSOR_PREFIX, offset + page.len())
    });
    let backwards_cursor = (offset > 0).then(|| {
        encode_aggregated_offset_cursor(
            AGGREGATED_THREAD_CURSOR_PREFIX,
            offset.saturating_sub(limit),
        )
    });

    serde_json::to_value(ThreadListResponse {
        data: page,
        next_cursor,
        backwards_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_loaded_thread_list_response(
    downstream: &GatewayV2DownstreamRouter,
    scope_registry: &GatewayScopeRegistry,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    log_degraded_multi_worker_thread_discovery(downstream, context, observability, &request.method);

    let params = request_params::<codex_app_server_protocol::ThreadLoadedListParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_LOADED_THREAD_CURSOR_PREFIX,
        "loaded thread",
    )?;
    let mut thread_ids = Vec::new();
    let mut seen = HashSet::new();

    for worker in &downstream.workers {
        let loaded_ids = collect_worker_paginated_data::<
            codex_app_server_protocol::ThreadLoadedListParams,
            ThreadLoadedListResponse,
            _,
        >(worker, request, |response| {
            (response.data, response.next_cursor)
        })
        .await?;
        for thread_id in loaded_ids {
            if scope_registry.thread_visible_to(context, &thread_id) {
                scope_registry.register_thread_worker_if_visible(
                    &thread_id,
                    context,
                    worker.worker_id,
                );
            }
            if scope_registry.thread_visible_to(context, &thread_id)
                && seen.insert(thread_id.clone())
            {
                thread_ids.push(thread_id);
            }
        }
    }

    let limit = params.limit.unwrap_or(thread_ids.len() as u32).max(1) as usize;
    let (start, end, next_cursor) = aggregated_page_bounds(
        thread_ids.len(),
        offset,
        limit,
        AGGREGATED_LOADED_THREAD_CURSOR_PREFIX,
    );

    serde_json::to_value(ThreadLoadedListResponse {
        data: thread_ids[start..end].to_vec(),
        next_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_apps_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let params = request_params::<AppsListParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_APPS_CURSOR_PREFIX,
        "apps",
    )?;
    let fanout_request = codex_app_server_protocol::JSONRPCRequest {
        id: request.id.clone(),
        method: request.method.clone(),
        params: Some(
            serde_json::to_value(AppsListParams {
                cursor: None,
                limit: None,
                thread_id: None,
                force_refetch: params.force_refetch,
            })
            .map_err(io::Error::other)?,
        ),
        trace: request.trace.clone(),
    };

    let mut apps = Vec::<AppInfo>::new();
    let mut seen_ids = HashSet::new();

    for worker in &downstream.workers {
        let worker_apps = collect_worker_paginated_data::<AppsListParams, AppsListResponse, _>(
            worker,
            &fanout_request,
            |response| (response.data, response.next_cursor),
        )
        .await?;
        for app in worker_apps {
            if seen_ids.insert(app.id.clone()) {
                apps.push(app);
            }
        }
    }

    let limit = params.limit.unwrap_or(apps.len() as u32).max(1) as usize;
    let (start, end, next_cursor) =
        aggregated_page_bounds(apps.len(), offset, limit, AGGREGATED_APPS_CURSOR_PREFIX);

    serde_json::to_value(AppsListResponse {
        data: apps[start..end].to_vec(),
        next_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_mcp_server_status_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let params = request_params::<ListMcpServerStatusParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_MCP_SERVER_STATUS_CURSOR_PREFIX,
        "mcp server status",
    )?;
    let fanout_request = codex_app_server_protocol::JSONRPCRequest {
        id: request.id.clone(),
        method: request.method.clone(),
        params: Some(
            serde_json::to_value(ListMcpServerStatusParams {
                cursor: None,
                limit: None,
                detail: params.detail,
                thread_id: params.thread_id.clone(),
            })
            .map_err(io::Error::other)?,
        ),
        trace: request.trace.clone(),
    };

    let mut statuses = Vec::<McpServerStatus>::new();
    let mut seen_names = HashSet::new();

    for worker in &downstream.workers {
        let worker_statuses = collect_worker_paginated_data::<
            ListMcpServerStatusParams,
            ListMcpServerStatusResponse,
            _,
        >(worker, &fanout_request, |response| {
            (response.data, response.next_cursor)
        })
        .await?;
        for status in worker_statuses {
            if seen_names.insert(status.name.clone()) {
                statuses.push(status);
            }
        }
    }

    let limit = params.limit.unwrap_or(statuses.len() as u32).max(1) as usize;
    let (start, end, next_cursor) = aggregated_page_bounds(
        statuses.len(),
        offset,
        limit,
        AGGREGATED_MCP_SERVER_STATUS_CURSOR_PREFIX,
    );

    serde_json::to_value(ListMcpServerStatusResponse {
        data: statuses[start..end].to_vec(),
        next_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_external_agent_config_detect_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut items = Vec::new();

    for worker in &downstream.workers {
        let response: ExternalAgentConfigDetectResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        for item in response.items {
            if !items.contains(&item) {
                items.push(item);
            }
        }
    }

    serde_json::to_value(ExternalAgentConfigDetectResponse { items }).map_err(io::Error::other)
}

pub(crate) async fn aggregate_account_read_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut primary_response = None;
    let mut requires_openai_auth = false;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response: GetAccountResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        requires_openai_auth |= response.requires_openai_auth;
        if index == 0 {
            primary_response = Some(response);
        }
    }

    let mut response = primary_response.ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })?;
    response.requires_openai_auth = requires_openai_auth;
    serde_json::to_value(response).map_err(io::Error::other)
}

pub(crate) async fn aggregate_get_auth_status_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut primary_response = None;
    let mut requires_openai_auth = None;

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response: GetAuthStatusResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        if let Some(requires_openai_auth_value) = response.requires_openai_auth {
            requires_openai_auth =
                Some(requires_openai_auth.unwrap_or(false) || requires_openai_auth_value);
        }
        if index == 0 {
            primary_response = Some(response);
        }
    }

    let mut response = primary_response.ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })?;
    response.requires_openai_auth = requires_openai_auth;
    serde_json::to_value(response).map_err(io::Error::other)
}

pub(crate) async fn aggregate_account_rate_limits_response(
    downstream: &GatewayV2DownstreamRouter,
    context: &GatewayRequestContext,
    observability: &GatewayObservability,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut primary_response = None;
    let mut aggregated_rate_limits_by_limit_id = HashMap::new();

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response: GetAccountRateLimitsResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        sync_worker_account_capacity_from_rate_limits_response(
            downstream,
            context,
            observability,
            worker.worker_id,
            &response,
        );
        if let Some(rate_limits_by_limit_id) = &response.rate_limits_by_limit_id {
            for (limit_id, snapshot) in rate_limits_by_limit_id {
                aggregated_rate_limits_by_limit_id
                    .entry(limit_id.clone())
                    .or_insert_with(|| snapshot.clone());
            }
        }
        if index == 0 {
            primary_response = Some(response);
        }
    }

    let mut response = primary_response.ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })?;
    response.rate_limits_by_limit_id = (!aggregated_rate_limits_by_limit_id.is_empty())
        .then_some(aggregated_rate_limits_by_limit_id);
    serde_json::to_value(response).map_err(io::Error::other)
}
