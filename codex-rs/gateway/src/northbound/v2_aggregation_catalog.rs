//! Catalog-style aggregation helpers for northbound v2 multi-worker fan-out.
//!
//! These helpers own the wider catalog and merged list responses that are not
//! part of the core thread/account aggregation path.

use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_pagination::AGGREGATED_EXPERIMENTAL_FEATURE_CURSOR_PREFIX;
use crate::northbound::v2_pagination::AGGREGATED_MODEL_CURSOR_PREFIX;
use crate::northbound::v2_pagination::aggregated_page_bounds;
use crate::northbound::v2_pagination::collect_worker_paginated_data;
use crate::northbound::v2_pagination::decode_aggregated_offset_cursor;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use crate::northbound::v2_wire::request_params;
use codex_app_server_protocol::CollaborationModeListResponse;
use codex_app_server_protocol::CollaborationModeMask;
use codex_app_server_protocol::ExperimentalFeature;
use codex_app_server_protocol::ExperimentalFeatureListParams;
use codex_app_server_protocol::ExperimentalFeatureListResponse;
use codex_app_server_protocol::FuzzyFileSearchResponse;
use codex_app_server_protocol::FuzzyFileSearchResult;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginMarketplaceEntry;
use codex_app_server_protocol::PluginSummary;
use codex_app_server_protocol::SkillsListEntry;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::ThreadRealtimeListVoicesResponse;
use codex_protocol::protocol::RealtimeVoice;
use codex_protocol::protocol::RealtimeVoicesList;
use serde_json::Value;
use std::collections::HashSet;
use std::io;

pub(crate) async fn aggregate_model_list_response_if_supported(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let params = request_params::<ModelListParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_MODEL_CURSOR_PREFIX,
        "model",
    )?;
    let fanout_request = codex_app_server_protocol::JSONRPCRequest {
        id: request.id.clone(),
        method: request.method.clone(),
        params: Some(
            serde_json::to_value(ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: params.include_hidden,
            })
            .map_err(io::Error::other)?,
        ),
        trace: request.trace.clone(),
    };

    let mut models = Vec::new();
    let mut seen_ids = HashSet::new();

    for worker in &downstream.workers {
        let worker_models = collect_worker_paginated_data::<ModelListParams, ModelListResponse, _>(
            worker,
            &fanout_request,
            |response| (response.data, response.next_cursor),
        )
        .await?;
        for model in worker_models {
            if seen_ids.insert(model.id.clone()) {
                models.push(model);
            }
        }
    }

    let limit = params.limit.unwrap_or(models.len() as u32).max(1) as usize;
    let (start, end, next_cursor) =
        aggregated_page_bounds(models.len(), offset, limit, AGGREGATED_MODEL_CURSOR_PREFIX);

    serde_json::to_value(ModelListResponse {
        data: models[start..end].to_vec(),
        next_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_skills_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut entries = Vec::<SkillsListEntry>::new();

    for worker in &downstream.workers {
        let response: SkillsListResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        for mut incoming in response.data {
            if let Some(existing) = entries.iter_mut().find(|entry| entry.cwd == incoming.cwd) {
                for skill in incoming.skills.drain(..) {
                    if !existing.skills.contains(&skill) {
                        existing.skills.push(skill);
                    }
                }
                for error in incoming.errors.drain(..) {
                    if !existing.errors.contains(&error) {
                        existing.errors.push(error);
                    }
                }
            } else {
                entries.push(incoming);
            }
        }
    }

    serde_json::to_value(SkillsListResponse { data: entries }).map_err(io::Error::other)
}

pub(crate) async fn aggregate_plugin_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut marketplaces = Vec::<PluginMarketplaceEntry>::new();
    let mut featured_plugin_ids = Vec::<String>::new();
    let mut marketplace_load_errors = Vec::new();

    for worker in &downstream.workers {
        let response: PluginListResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        for marketplace in response.marketplaces {
            merge_plugin_marketplace(&mut marketplaces, marketplace);
        }
        for plugin_id in response.featured_plugin_ids {
            if !featured_plugin_ids.contains(&plugin_id) {
                featured_plugin_ids.push(plugin_id);
            }
        }
        for load_error in response.marketplace_load_errors {
            if !marketplace_load_errors.contains(&load_error) {
                marketplace_load_errors.push(load_error);
            }
        }
    }

    serde_json::to_value(PluginListResponse {
        marketplaces,
        marketplace_load_errors,
        featured_plugin_ids,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_realtime_list_voices_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut primary_response = None;
    let mut merged_v1 = Vec::<RealtimeVoice>::new();
    let mut merged_v2 = Vec::<RealtimeVoice>::new();

    for (index, worker) in downstream.workers.iter().enumerate() {
        let response: ThreadRealtimeListVoicesResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        merge_realtime_voices(&mut merged_v1, &response.voices.v1);
        merge_realtime_voices(&mut merged_v2, &response.voices.v2);
        if index == 0 {
            primary_response = Some(response);
        }
    }

    let mut response = primary_response.ok_or_else(|| {
        io::Error::other("gateway v2 connection has no downstream app-server sessions")
    })?;
    response.voices = RealtimeVoicesList {
        v1: merged_v1,
        v2: merged_v2,
        default_v1: response.voices.default_v1,
        default_v2: response.voices.default_v2,
    };
    serde_json::to_value(response).map_err(io::Error::other)
}

pub(crate) async fn aggregate_fuzzy_file_search_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut files = Vec::<FuzzyFileSearchResult>::new();

    for worker in &downstream.workers {
        let response: FuzzyFileSearchResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        for incoming in response.files {
            if let Some(existing) = files.iter_mut().find(|file| {
                file.root == incoming.root
                    && file.path == incoming.path
                    && file.match_type == incoming.match_type
            }) {
                if incoming.score > existing.score {
                    *existing = incoming;
                }
            } else {
                files.push(incoming);
            }
        }
    }

    files.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.root.cmp(&b.root))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.file_name.cmp(&b.file_name))
    });

    serde_json::to_value(FuzzyFileSearchResponse { files }).map_err(io::Error::other)
}

pub(crate) fn merge_realtime_voices(target: &mut Vec<RealtimeVoice>, incoming: &[RealtimeVoice]) {
    for voice in incoming {
        if !target.contains(voice) {
            target.push(*voice);
        }
    }
}

pub(crate) async fn aggregate_experimental_feature_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let params = request_params::<ExperimentalFeatureListParams>(request)?;
    let offset = decode_aggregated_offset_cursor(
        params.cursor.as_deref(),
        AGGREGATED_EXPERIMENTAL_FEATURE_CURSOR_PREFIX,
        "experimental feature",
    )?;
    let fanout_request = codex_app_server_protocol::JSONRPCRequest {
        id: request.id.clone(),
        method: request.method.clone(),
        params: Some(
            serde_json::to_value(ExperimentalFeatureListParams {
                cursor: None,
                limit: None,
                thread_id: params.thread_id.clone(),
            })
            .map_err(io::Error::other)?,
        ),
        trace: request.trace.clone(),
    };

    let mut features = Vec::<ExperimentalFeature>::new();

    for worker in &downstream.workers {
        let worker_features = collect_worker_paginated_data::<
            ExperimentalFeatureListParams,
            ExperimentalFeatureListResponse,
            _,
        >(worker, &fanout_request, |response| {
            (response.data, response.next_cursor)
        })
        .await?;
        for incoming in worker_features {
            if let Some(existing) = features
                .iter_mut()
                .find(|feature| feature.name == incoming.name)
            {
                existing.enabled |= incoming.enabled;
                existing.default_enabled |= incoming.default_enabled;
            } else {
                features.push(incoming);
            }
        }
    }

    features.sort_by(|a, b| a.name.cmp(&b.name));
    let limit = params.limit.unwrap_or(features.len() as u32).max(1) as usize;
    let (start, end, next_cursor) = aggregated_page_bounds(
        features.len(),
        offset,
        limit,
        AGGREGATED_EXPERIMENTAL_FEATURE_CURSOR_PREFIX,
    );

    serde_json::to_value(ExperimentalFeatureListResponse {
        data: features[start..end].to_vec(),
        next_cursor,
    })
    .map_err(io::Error::other)
}

pub(crate) async fn aggregate_collaboration_mode_list_response(
    downstream: &GatewayV2DownstreamRouter,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> io::Result<Value> {
    downstream.ensure_all_configured_workers_present_for(&request.method)?;

    let mut modes = Vec::<CollaborationModeMask>::new();

    for worker in &downstream.workers {
        let response: CollaborationModeListResponse = worker
            .request_handle
            .request_typed(jsonrpc_request_to_client_request(request.clone())?)
            .await
            .map_err(io::Error::other)?;
        for incoming in response.data {
            if !modes.iter().any(|mode| mode.name == incoming.name) {
                modes.push(incoming);
            }
        }
    }

    modes.sort_by(|a, b| a.name.cmp(&b.name));

    serde_json::to_value(CollaborationModeListResponse { data: modes }).map_err(io::Error::other)
}

pub(crate) fn merge_plugin_marketplace(
    marketplaces: &mut Vec<PluginMarketplaceEntry>,
    mut incoming: PluginMarketplaceEntry,
) {
    if let Some(existing) = marketplaces
        .iter_mut()
        .find(|entry| entry.name == incoming.name && entry.path == incoming.path)
    {
        for plugin in incoming.plugins.drain(..) {
            merge_plugin_summary(&mut existing.plugins, plugin);
        }
    } else {
        marketplaces.push(incoming);
    }
}

pub(crate) fn merge_plugin_summary(plugins: &mut Vec<PluginSummary>, incoming: PluginSummary) {
    if let Some(existing) = plugins.iter_mut().find(|entry| entry.id == incoming.id) {
        let was_installed = existing.installed;
        if incoming.installed && !was_installed {
            *existing = incoming;
        } else {
            existing.installed |= incoming.installed;
            existing.enabled |= incoming.enabled;
        }
    } else {
        plugins.push(incoming);
    }
}
