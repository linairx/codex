use crate::northbound::v2_account_capacity::fail_closed_active_thread_request_if_account_exhausted;
use crate::northbound::v2_account_capacity::route_thread_start_request_with_account_capacity_failover;
use crate::northbound::v2_aggregation::aggregate_account_rate_limits_response;
use crate::northbound::v2_aggregation::aggregate_account_read_response;
use crate::northbound::v2_aggregation::aggregate_apps_list_response;
use crate::northbound::v2_aggregation::aggregate_collaboration_mode_list_response;
use crate::northbound::v2_aggregation::aggregate_experimental_feature_list_response;
use crate::northbound::v2_aggregation::aggregate_external_agent_config_detect_response;
use crate::northbound::v2_aggregation::aggregate_fuzzy_file_search_response;
use crate::northbound::v2_aggregation::aggregate_get_auth_status_response;
use crate::northbound::v2_aggregation::aggregate_loaded_thread_list_response;
use crate::northbound::v2_aggregation::aggregate_mcp_server_status_list_response;
use crate::northbound::v2_aggregation::aggregate_model_list_response_if_supported;
use crate::northbound::v2_aggregation::aggregate_plugin_list_response;
use crate::northbound::v2_aggregation::aggregate_realtime_list_voices_response;
use crate::northbound::v2_aggregation::aggregate_skills_list_response;
use crate::northbound::v2_aggregation::aggregate_thread_list_response;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_connection_runtime::log_fail_closed_multi_worker_request;
use crate::northbound::v2_request_routing::fanout_fs_unwatch_request;
use crate::northbound::v2_request_routing::fanout_fs_watch_request;
use crate::northbound::v2_request_routing::fanout_mutating_connection_request;
use crate::northbound::v2_request_routing::first_successful_connection_request;
use crate::northbound::v2_request_routing::is_multi_worker_fanout_login_request;
use crate::northbound::v2_request_routing::route_config_read_request_if_supported;
use crate::northbound::v2_request_routing_handoff::first_successful_visible_thread_read_request;
use crate::northbound::v2_request_routing_handoff::recover_visible_thread_worker_route as recover_visible_thread_worker_route_impl;
use crate::northbound::v2_request_routing_handoff::route_thread_id_request_with_account_handoff;
use crate::northbound::v2_request_routing_handoff::thread_id_account_handoff_metrics;
use crate::northbound::v2_request_routing_path_handoff::first_successful_scoped_path_thread_request;
use crate::northbound::v2_routing::requires_primary_worker_route;
use crate::northbound::v2_routing::worker_for_request;
use crate::northbound::v2_scope::apply_response_scope_policy;
use crate::northbound::v2_scope::request_thread_id;
use crate::northbound::v2_scope::request_thread_path;
use crate::northbound::v2_wire::jsonrpc_request_to_client_request;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use serde_json::Value;
use std::io;

pub(crate) async fn handle_client_request(
    downstream: &mut GatewayV2DownstreamRouter,
    connection: &GatewayV2ConnectionContext<'_>,
    request: JSONRPCRequest,
) -> io::Result<Result<Value, JSONRPCErrorError>> {
    let request_method = request.method.clone();
    let result = async {
        if downstream.reconnect_state.is_some() {
            downstream
                .reconnect_missing_workers_for_request(connection.observability)
                .await;
        }

        if downstream.multi_worker_topology()
            && let Some(thread_id) = request_thread_id(&request)
            && !requires_primary_worker_route(&request)
            && connection
                .scope_registry
                .thread_visible_to(connection.request_context, thread_id)
            && connection
                .scope_registry
                .thread_worker_id(thread_id)
                .is_none()
        {
            downstream.ensure_all_configured_workers_present_for(&request.method)?;
            recover_visible_thread_worker_route_impl(
                downstream,
                connection.scope_registry,
                connection.request_context,
                connection.observability,
                thread_id,
            )
            .await?;
        }

        if downstream.multi_worker_topology() {
            if is_multi_worker_fanout_login_request(&request) {
                return fanout_mutating_connection_request(downstream, request).await;
            }

            match request.method.as_str() {
                "thread/resume" | "thread/fork" | "getConversationSummary"
                    if request_thread_path(&request).is_some() =>
                {
                    return first_successful_scoped_path_thread_request(
                        downstream,
                        connection.scope_registry,
                        connection.request_context,
                        connection.observability,
                        request,
                    )
                    .await;
                }
                "fs/watch" => return fanout_fs_watch_request(downstream, request).await,
                "fs/unwatch" => return fanout_fs_unwatch_request(downstream, request).await,
                "app/list" if request_thread_id(&request).is_none() => {
                    return aggregate_apps_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "mcpServerStatus/list" => {
                    return aggregate_mcp_server_status_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "thread/list" => {
                    return aggregate_thread_list_response(
                        downstream,
                        connection.scope_registry,
                        connection.request_context,
                        connection.observability,
                        &request,
                    )
                    .await
                    .map(Ok);
                }
                "thread/loaded/list" => {
                    return aggregate_loaded_thread_list_response(
                        downstream,
                        connection.scope_registry,
                        connection.request_context,
                        connection.observability,
                        &request,
                    )
                    .await
                    .map(Ok);
                }
                "account/read" => {
                    return aggregate_account_read_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "getAuthStatus" => {
                    return aggregate_get_auth_status_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "account/rateLimits/read" => {
                    return aggregate_account_rate_limits_response(
                        downstream,
                        connection.request_context,
                        connection.observability,
                        &request,
                    )
                    .await
                    .map(Ok);
                }
                "model/list" => {
                    return aggregate_model_list_response_if_supported(downstream, &request)
                        .await
                        .map(Ok);
                }
                "externalAgentConfig/detect" => {
                    return aggregate_external_agent_config_detect_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "skills/list" => {
                    return aggregate_skills_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "experimentalFeature/list" => {
                    return aggregate_experimental_feature_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "config/read" => {
                    return route_config_read_request_if_supported(downstream, request).await;
                }
                "collaborationMode/list" => {
                    return aggregate_collaboration_mode_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "plugin/list" => {
                    return aggregate_plugin_list_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "thread/realtime/listVoices" => {
                    return aggregate_realtime_list_voices_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "fuzzyFileSearch" => {
                    return aggregate_fuzzy_file_search_response(downstream, &request)
                        .await
                        .map(Ok);
                }
                "plugin/read" | "plugin/install" | "plugin/uninstall" => {
                    return first_successful_connection_request(downstream, request).await;
                }
                "gitDiffToRemote" => {
                    return first_successful_connection_request(downstream, request).await;
                }
                "mcpServer/oauth/login" => {
                    return first_successful_connection_request(downstream, request).await;
                }
                "thread/read"
                    if request_thread_id(&request).is_some_and(|thread_id| {
                        connection
                            .scope_registry
                            .thread_visible_to(connection.request_context, thread_id)
                            && connection
                                .scope_registry
                                .thread_worker_id(thread_id)
                                .is_none()
                    }) =>
                {
                    return first_successful_visible_thread_read_request(
                        downstream,
                        connection.scope_registry,
                        connection.request_context,
                        request,
                    )
                    .await;
                }
                "externalAgentConfig/import"
                | "marketplace/add"
                | "skills/config/write"
                | "experimentalFeature/enablement/set"
                | "config/mcpServer/reload"
                | "config/value/write"
                | "config/batchWrite"
                | "memory/reset"
                | "account/logout" => {
                    return fanout_mutating_connection_request(downstream, request).await;
                }
                _ => {}
            }
        }

        if request.method == "thread/start" {
            return route_thread_start_request_with_account_capacity_failover(
                downstream, connection, request,
            )
            .await;
        }

        if thread_id_account_handoff_metrics(request.method.as_str()).is_some()
            && request_thread_id(&request).is_some_and(|thread_id| {
                connection
                    .scope_registry
                    .thread_visible_to(connection.request_context, thread_id)
            })
        {
            return route_thread_id_request_with_account_handoff(downstream, connection, request)
                .await;
        }

        fail_closed_active_thread_request_if_account_exhausted(downstream, connection, &request)?;

        let worker = worker_for_request(
            downstream,
            connection.scope_registry,
            connection.request_context,
            &request,
        )?
        .clone();
        let worker_account_id = worker
            .worker_id
            .and_then(|worker_id| downstream.worker_account_id(worker_id));
        let method = request.method.clone();
        let client_request = jsonrpc_request_to_client_request(request)?;
        let response = worker.request_handle.request(client_request).await?;
        Ok(match response {
            Ok(result) => Ok(apply_response_scope_policy(
                connection.scope_registry,
                connection.request_context,
                Some(connection.observability),
                &method,
                worker.worker_id,
                worker_account_id,
                result,
            )?),
            Err(error) => Err(error),
        })
    }
    .await;

    if let Err(err) = &result {
        log_fail_closed_multi_worker_request(downstream, connection, request_method.as_str(), err);
    }

    result
}
