use super::*;
use crate::northbound::v2_aggregation::aggregate_loaded_thread_list_response;
use crate::northbound::v2_aggregation::aggregate_mcp_server_status_list_response;
use crate::northbound::v2_aggregation::aggregate_thread_list_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_loaded_thread_list_response_backfills_multi_worker_routes_for_visible_threads() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/loaded/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
        }),
        serde_json::json!({
            "data": ["thread-worker-a"],
            "nextCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/loaded/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
        }),
        serde_json::json!({
            "data": ["thread-worker-b"],
            "nextCursor": null,
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
    scope_registry.register_thread("thread-worker-b".to_string(), context);
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_a,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_b,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
        ],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let router = GatewayV2DownstreamRouter::connect(
        &session_factory,
        &initialize_params,
        &GatewayRequestContext::default(),
    )
    .await
    .expect("downstream router should connect");
    let loaded = aggregate_loaded_thread_list_response(
        &router,
        &scope_registry,
        &GatewayRequestContext::default(),
        &GatewayObservability::default(),
        &JSONRPCRequest {
            id: RequestId::String("thread-loaded-list".to_string()),
            method: "thread/loaded/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            trace: None,
        },
    )
    .await
    .expect("loaded thread list aggregation should succeed");
    let loaded: codex_app_server_protocol::ThreadLoadedListResponse =
        serde_json::from_value(loaded).expect("loaded thread list response should decode");
    assert_eq!(loaded.next_cursor, None);
    assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}

#[tokio::test]
async fn aggregate_thread_list_response_drains_downstream_pages_before_gateway_pagination() {
    let worker = start_mock_remote_server_for_paginated_passthrough_requests(
        "thread/list",
        vec![
            (
                serde_json::json!({
                    "cursor": null,
                    "limit": null,
                    "sortKey": null,
                    "sortDirection": null,
                    "modelProviders": null,
                    "sourceKinds": null,
                    "archived": null,
                    "cwd": null,
                    "searchTerm": null,
                }),
                serde_json::json!({
                    "data": [{
                        "id": "thread-a",
                        "sessionId": "thread-a",
                        "forkedFromId": null,
                        "preview": "",
                        "ephemeral": true,
                        "modelProvider": "openai",
                        "createdAt": 1,
                        "updatedAt": 1,
                        "status": { "type": "idle" },
                        "path": null,
                        "cwd": "/tmp/thread-a",
                        "cliVersion": "0.0.0-test",
                        "source": "cli",
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": "Thread A",
                        "turns": [],
                    }],
                    "nextCursor": "worker-thread-page-2",
                    "backwardsCursor": null,
                }),
            ),
            (
                serde_json::json!({
                    "cursor": "worker-thread-page-2",
                    "limit": null,
                    "sortKey": null,
                    "sortDirection": null,
                    "modelProviders": null,
                    "sourceKinds": null,
                    "archived": null,
                    "cwd": null,
                    "searchTerm": null,
                }),
                serde_json::json!({
                    "data": [{
                        "id": "thread-b",
                        "sessionId": "thread-b",
                        "forkedFromId": null,
                        "preview": "",
                        "ephemeral": true,
                        "modelProvider": "openai",
                        "createdAt": 2,
                        "updatedAt": 2,
                        "status": { "type": "idle" },
                        "path": null,
                        "cwd": "/tmp/thread-b",
                        "cliVersion": "0.0.0-test",
                        "source": "cli",
                        "agentNickname": null,
                        "agentRole": null,
                        "gitInfo": null,
                        "name": "Thread B",
                        "turns": [],
                    }],
                    "nextCursor": null,
                    "backwardsCursor": null,
                }),
            ),
        ],
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-a".to_string(), context.clone());
    scope_registry.register_thread("thread-b".to_string(), context.clone());
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![RemoteAppServerConnectArgs {
            endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                websocket_url: worker,
                auth_token: None,
            },
            client_name: "codex-gateway".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 4,
        }],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let router = GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
        .await
        .expect("downstream router should connect");
    let result = aggregate_thread_list_response(
        &router,
        &scope_registry,
        &context,
        &GatewayObservability::default(),
        &JSONRPCRequest {
            id: RequestId::String("thread-list".to_string()),
            method: "thread/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "offset:1",
                "limit": 1,
                "sortKey": null,
                "sortDirection": null,
                "modelProviders": null,
                "sourceKinds": null,
                "archived": null,
                "cwd": null,
                "searchTerm": null,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread list aggregation should succeed");
    let result: ThreadListResponse =
        serde_json::from_value(result).expect("thread list response should decode");
    assert_eq!(
        result
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-a"]
    );
    assert_eq!(result.next_cursor, None);
}

#[tokio::test]
async fn aggregate_loaded_thread_list_response_drains_downstream_pages_before_gateway_pagination() {
    let worker = start_mock_remote_server_for_paginated_passthrough_requests(
        "thread/loaded/list",
        vec![
            (
                serde_json::json!({
                    "cursor": null,
                    "limit": null,
                }),
                serde_json::json!({
                    "data": ["thread-a"],
                    "nextCursor": "worker-loaded-page-2",
                }),
            ),
            (
                serde_json::json!({
                    "cursor": "worker-loaded-page-2",
                    "limit": null,
                }),
                serde_json::json!({
                    "data": ["thread-b"],
                    "nextCursor": null,
                }),
            ),
        ],
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-a".to_string(), context.clone());
    scope_registry.register_thread("thread-b".to_string(), context.clone());
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![RemoteAppServerConnectArgs {
            endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                websocket_url: worker,
                auth_token: None,
            },
            client_name: "codex-gateway".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 4,
        }],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let router = GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
        .await
        .expect("downstream router should connect");
    let result = aggregate_loaded_thread_list_response(
        &router,
        &scope_registry,
        &context,
        &GatewayObservability::default(),
        &JSONRPCRequest {
            id: RequestId::String("thread-loaded-list".to_string()),
            method: "thread/loaded/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "loaded-thread-offset:1",
                "limit": 1,
            })),
            trace: None,
        },
    )
    .await
    .expect("loaded thread list aggregation should succeed");
    let result: codex_app_server_protocol::ThreadLoadedListResponse =
        serde_json::from_value(result).expect("loaded thread list response should decode");
    assert_eq!(result.data, vec!["thread-b"]);
    assert_eq!(result.next_cursor, None);
}

#[tokio::test]
async fn aggregate_mcp_server_status_list_response_drains_downstream_pages_before_gateway_pagination()
 {
    let worker = start_mock_remote_server_for_paginated_passthrough_requests(
        "mcpServerStatus/list",
        vec![
            (
                serde_json::json!({
                    "cursor": null,
                    "limit": null,
                    "detail": "toolsAndAuthOnly",
                }),
                serde_json::json!({
                    "data": [{
                        "name": "mcp-a",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "unsupported",
                    }],
                    "nextCursor": "worker-mcp-page-2",
                }),
            ),
            (
                serde_json::json!({
                    "cursor": "worker-mcp-page-2",
                    "limit": null,
                    "detail": "toolsAndAuthOnly",
                }),
                serde_json::json!({
                    "data": [{
                        "name": "mcp-b",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "unsupported",
                    }],
                    "nextCursor": null,
                }),
            ),
        ],
    )
    .await;
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![RemoteAppServerConnectArgs {
            endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                websocket_url: worker,
                auth_token: None,
            },
            client_name: "codex-gateway".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: false,
            mcp_server_openai_form_elicitation: false,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 4,
        }],
        test_initialize_response().await,
    );
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let router = GatewayV2DownstreamRouter::connect(
        &session_factory,
        &initialize_params,
        &GatewayRequestContext::default(),
    )
    .await
    .expect("downstream router should connect");
    let result = aggregate_mcp_server_status_list_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("mcp-statuses".to_string()),
            method: "mcpServerStatus/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "mcp-status-offset:1",
                "limit": 1,
                "detail": "toolsAndAuthOnly",
            })),
            trace: None,
        },
    )
    .await
    .expect("mcp status aggregation should succeed");
    let result: ListMcpServerStatusResponse =
        serde_json::from_value(result).expect("mcp status response should decode");
    assert_eq!(
        result
            .data
            .iter()
            .map(|status| status.name.as_str())
            .collect::<Vec<_>>(),
        vec!["mcp-b"]
    );
    assert_eq!(result.next_cursor, None);
}
