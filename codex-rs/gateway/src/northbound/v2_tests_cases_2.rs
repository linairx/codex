use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_2_late.rs"]
mod v2_tests_cases_2_late;

#[path = "v2_tests_cases_2_account_and_collaboration.rs"]
mod v2_tests_cases_2_account_and_collaboration;

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
    let loaded = super::super::super::aggregate_loaded_thread_list_response(
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
    let result = super::super::super::aggregate_thread_list_response(
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
    let result = super::super::super::aggregate_loaded_thread_list_response(
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
    let result = super::super::super::aggregate_mcp_server_status_list_response(
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

#[tokio::test]
async fn handle_client_request_probes_visible_thread_read_across_workers_when_route_missing() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async(async {
        let thread_read_params = serde_json::json!({
            "threadId": "thread-worker-b",
            "includeTurns": false,
        });
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: "thread not found: thread-worker-b".to_string(),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_thread_list_and_read(
            "thread-worker-b",
            "Worker B thread",
            "/tmp/worker-b",
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-visible".to_string(),
            project_id: Some("project-visible".to_string()),
        };
        scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
        let mut router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");
        let admission = GatewayAdmissionController::default();
        let observability = GatewayObservability::new(Some(metrics.clone()), false);
        let connection = GatewayV2ConnectionContext {
            admission: &admission,
            observability: &observability,
            scope_registry: &scope_registry,
            request_context: &context,
            client_send_timeout: Duration::from_secs(10),
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            opt_out_notification_methods: HashSet::new(),
        };

        let result = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("thread-read".to_string()),
                method: "thread/read".to_string(),
                params: Some(thread_read_params),
                trace: None,
            },
        )
        .await
        .expect("thread/read should reach downstream workers")
        .expect("thread/read should succeed through probed worker");

        assert_eq!(
            result,
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Worker B thread",
                    "cwd": "/tmp/worker-b",
                },
            })
        );
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    })
    .await;

    assert!(
        logs.contains("recovered missing visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread-worker-b"));
    assert!(logs.contains("worker_id=Some(1)"));
    assert_v2_thread_route_recovery_metric(&metrics, "success");
    assert_no_v2_metric(&metrics, "gateway_v2_fail_closed_requests");
    assert_no_v2_metric(&metrics, "gateway_v2_upstream_request_failures");
}

#[tokio::test]
async fn visible_thread_route_recovery_fails_closed_during_reconnect_backoff() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "thread/read",
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Wrong worker thread",
                    "cwd": "/tmp/worker-a",
                },
            }),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "thread/read",
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Worker B thread",
                    "cwd": "/tmp/worker-b",
                },
            }),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the owning worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

    let metrics = in_memory_metrics();
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let err = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .await
    .expect_err("visible thread route recovery should fail closed during reconnect backoff");

    assert_eq!(
        err.to_string(),
        "required worker routes are unavailable for thread/read: [1]"
    );
    assert_eq!(router.worker_count(), 1);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), None);
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    assert_v2_fail_closed_request_metric(&metrics, "thread/read", true);
}

#[tokio::test]
async fn recover_visible_thread_worker_route_logs_when_probe_finds_no_owner() {
    let metrics = in_memory_metrics();
    let logs = capture_logs_async(async {
        let thread_read_params = serde_json::json!({
            "threadId": "thread-missing",
            "includeTurns": false,
        });
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: "thread not found: thread-missing".to_string(),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_error(
            "thread/read",
            thread_read_params,
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: "thread not found: thread-missing".to_string(),
                data: None,
            },
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-visible".to_string(),
            project_id: Some("project-visible".to_string()),
        };
        scope_registry.register_thread("thread-missing".to_string(), context.clone());

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
        let router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");

        super::super::super::recover_visible_thread_worker_route(
            &router,
            &scope_registry,
            &context,
            &GatewayObservability::new(Some(metrics.clone()), false),
            "thread-missing",
        )
        .await
        .expect("thread route probe should complete");

        assert_eq!(scope_registry.thread_worker_id("thread-missing"), None);
    })
    .await;

    assert!(
        logs.contains("failed to recover visible thread route via downstream thread/read probe")
    );
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread-missing"));
    assert!(logs.contains("attempted_worker_ids=[Some(0), Some(1)]"));
    assert!(logs.contains("attempted_worker_websocket_urls=["));
    assert_v2_thread_route_recovery_metric(&metrics, "miss");
    assert_no_v2_metric(&metrics, "gateway_v2_fail_closed_requests");
    assert_no_v2_metric(&metrics, "gateway_v2_upstream_request_failures");
}

#[tokio::test]
async fn handle_client_request_recovers_visible_thread_route_before_resume() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/read",
        serde_json::json!({
            "threadId": "thread-worker-b",
            "includeTurns": false,
        }),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_thread_list_and_read(
        "thread-worker-b",
        "Worker B thread",
        "/tmp/worker-b",
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-b".to_string(), context.clone());

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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let result = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume".to_string()),
            method: "thread/resume".to_string(),
            params: Some(thread_resume_params),
            trace: None,
        },
    )
    .await
    .expect("thread/resume should reach downstream workers")
    .expect("thread/resume should succeed through recovered worker route");

    assert_eq!(
        result,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}

#[tokio::test]
async fn thread_id_resume_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(thread_resume_params),
                    trace: None,
                },
            )
            .await
            .expect("thread/resume should try a replacement worker")
            .expect("thread/resume should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_resume_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_resume_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("thread_resume_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_resume_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_id_resume_rejects_replacement_worker_returning_wrong_thread() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong restored thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume".to_string()),
            method: "thread/resume".to_string(),
            params: Some(thread_resume_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/resume should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), None);
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_resume_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_resume_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("thread_resume_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_resume_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread resume handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_read_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_read_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "includeTurns": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_c = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker C restored thread",
                "cwd": "/tmp/worker-c",
            },
        }),
    )
    .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
        (worker_c.clone(), Some("acct-c".to_string())),
    ]));
    worker_health.mark_unhealthy(0, Some("socket closed".to_string()));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_c,
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
        vec![
            Some("acct-a".to_string()),
            Some("acct-b".to_string()),
            Some("acct-c".to_string()),
        ],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(thread_read_params),
            trace: None,
        },
    )
    .await
    .expect("thread/read should try a replacement worker")
    .expect("thread/read should restore from replacement worker");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(2));
    assert_v2_account_capacity_event_metrics(&metrics, &[(2, "thread_read_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_read_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(2, [("thread_read_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_read_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(2));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread read handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/read",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-c",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_read_rejects_replacement_worker_returning_wrong_thread() {
    let thread_read_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "includeTurns": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(thread_read_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/read should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_read_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_read_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("thread_read_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_read_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread read handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/read",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_unarchive_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_unarchive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/unarchive",
        thread_unarchive_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-unarchive".to_string()),
            method: "thread/unarchive".to_string(),
            params: Some(thread_unarchive_params),
            trace: None,
        },
    )
    .await
    .expect("thread/unarchive should try a replacement worker")
    .expect("thread/unarchive should restore from replacement worker");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_unarchive_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread unarchive handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/unarchive",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_unarchive_rejects_replacement_worker_returning_wrong_thread() {
    let thread_unarchive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/unarchive",
        thread_unarchive_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-unarchive".to_string()),
            method: "thread/unarchive".to_string(),
            params: Some(thread_unarchive_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/unarchive should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_unarchive_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread unarchive handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/unarchive",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_metadata_update_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_metadata_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "gitInfo": {
            "sha": "abc123",
            "branch": "main",
            "originUrl": null,
        },
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/metadata/update",
        thread_metadata_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
                "gitInfo": {
                    "commitHash": "abc123",
                    "branchName": "main",
                    "remoteUrl": null,
                },
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-metadata-update".to_string()),
            method: "thread/metadata/update".to_string(),
            params: Some(thread_metadata_params),
            trace: None,
        },
    )
    .await
    .expect("thread/metadata/update should try a replacement worker")
    .expect("thread/metadata/update should restore from replacement worker");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "thread_metadata_update_handoff_success")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread metadata update handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/metadata/update",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_metadata_update_rejects_replacement_worker_returning_wrong_thread() {
    let thread_metadata_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "gitInfo": {
            "sha": "abc123",
            "branch": "main",
            "originUrl": null,
        },
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/metadata/update",
        thread_metadata_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-metadata-update".to_string()),
            method: "thread/metadata/update".to_string(),
            params: Some(thread_metadata_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/metadata/update should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/metadata/update, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "thread_metadata_update_handoff_failure")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread metadata update handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/metadata/update",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/metadata/update, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_rollback_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_rollback_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "numTurns": 2,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/rollback",
        thread_rollback_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "turns": [],
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-rollback".to_string()),
            method: "thread/rollback".to_string(),
            params: Some(thread_rollback_params),
            trace: None,
        },
    )
    .await
    .expect("thread/rollback should try a replacement worker")
    .expect("thread/rollback should restore from replacement worker");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_rollback_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread rollback handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/rollback",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_rollback_rejects_replacement_worker_returning_wrong_thread() {
    let thread_rollback_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "numTurns": 2,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/rollback",
        thread_rollback_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "turns": [],
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-rollback".to_string()),
            method: "thread/rollback".to_string(),
            params: Some(thread_rollback_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/rollback should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/rollback, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_rollback_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread rollback handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/rollback",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/rollback, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_archive_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_archive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/archive",
        thread_archive_params.clone(),
        serde_json::json!({}),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-archive".to_string()),
            method: "thread/archive".to_string(),
            params: Some(thread_archive_params),
            trace: None,
        },
    )
    .await
    .expect("thread/archive should try a replacement worker")
    .expect("thread/archive should restore from replacement worker");

    assert_eq!(response, serde_json::json!({}));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_archive_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread archive handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/archive",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_archive_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_archive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/archive",
        thread_archive_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-archive".to_string()),
            method: "thread/archive".to_string(),
            params: Some(thread_archive_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/archive should fail closed without replacement");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/archive, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_archive_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread archive handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/archive",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/archive, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_turns_list_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_turns_list_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "cursor": null,
        "limit": 20,
        "sortDirection": "desc",
    });
    let turns_response = serde_json::json!({
        "data": [{
            "id": "turn-worker-b",
            "items": [],
            "status": "completed",
            "error": null,
            "startedAt": 1,
            "completedAt": 2,
            "durationMs": 1,
        }],
        "nextCursor": null,
        "backwardsCursor": null,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/turns/list",
        thread_turns_list_params.clone(),
        turns_response.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-turns-list".to_string()),
            method: "thread/turns/list".to_string(),
            params: Some(thread_turns_list_params),
            trace: None,
        },
    )
    .await
    .expect("thread/turns/list should try a replacement worker")
    .expect("thread/turns/list should restore from replacement worker");

    assert_eq!(response, turns_response);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_turns_list_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread turns list handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/turns/list",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_turns_list_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_turns_list_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "cursor": null,
        "limit": 20,
        "sortDirection": "desc",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/turns/list",
        thread_turns_list_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-turns-list".to_string()),
            method: "thread/turns/list".to_string(),
            params: Some(thread_turns_list_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/turns/list should fail closed without replacement");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_turns_list_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread turns list handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/turns/list",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_increment_elicitation_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_increment_elicitation_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let increment_response = serde_json::json!({
        "count": 2,
        "paused": true,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/increment_elicitation",
        thread_increment_elicitation_params.clone(),
        increment_response.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-increment-elicitation".to_string()),
            method: "thread/increment_elicitation".to_string(),
            params: Some(thread_increment_elicitation_params),
            trace: None,
        },
    )
    .await
    .expect("thread/increment_elicitation should try a replacement worker")
    .expect("thread/increment_elicitation should restore from replacement worker");

    assert_eq!(response, increment_response);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "thread_increment_elicitation_handoff_success")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread increment elicitation handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/increment_elicitation",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_increment_elicitation_records_handoff_failure_when_no_replacement_restores_context()
{
    let thread_increment_elicitation_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/increment_elicitation",
        thread_increment_elicitation_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-increment-elicitation".to_string()),
            method: "thread/increment_elicitation".to_string(),
            params: Some(thread_increment_elicitation_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/increment_elicitation should fail closed without replacement");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "thread_increment_elicitation_handoff_failure")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread increment elicitation handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/increment_elicitation",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn additional_thread_id_controls_use_replacement_worker_when_pinned_account_is_exhausted() {
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            serde_json::json!({}),
            "thread_name_set_handoff_success",
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            serde_json::json!({}),
            "thread_memory_mode_set_handoff_success",
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 1,
                "paused": false,
            }),
            "thread_decrement_elicitation_handoff_success",
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            }),
            serde_json::json!({}),
            "thread_inject_items_handoff_success",
        ),
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
            "thread_unsubscribe_handoff_success",
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
            "thread_compact_start_handoff_success",
        ),
    ];

    for (method, request_id, params, expected_response, success_metric) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            expected_response.clone(),
        )
        .await;
        let worker_b = start_mock_remote_server_for_idle_session().await;
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (worker_a.clone(), Some("acct-a".to_string())),
            (worker_b.clone(), Some("acct-b".to_string())),
        ]));
        worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );

        let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
            vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
        )
        .with_worker_health(worker_health);
        let initialize_params = InitializeParams {
            client_info: ClientInfo {
                name: "codex-tui".to_string(),
                title: None,
                version: "0.0.0-test".to_string(),
            },
            capabilities: None,
        };
        let mut router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");
        let admission = GatewayAdmissionController::default();
        let metrics = in_memory_metrics();
        let (operator_events_tx, _) = broadcast::channel(4);
        let mut operator_events_rx = operator_events_tx.subscribe();
        let observability = GatewayObservability::new(Some(metrics.clone()), false)
            .with_operator_events(operator_events_tx);
        let connection = GatewayV2ConnectionContext {
            admission: &admission,
            observability: &observability,
            scope_registry: &scope_registry,
            request_context: &context,
            client_send_timeout: Duration::from_secs(10),
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            opt_out_notification_methods: HashSet::new(),
        };

        let response = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(request_id.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .unwrap_or_else(|_| panic!("{method} should try a replacement worker"))
        .unwrap_or_else(|_| panic!("{method} should restore from replacement worker"));

        assert_eq!(response, expected_response);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
        assert_v2_account_capacity_event_metrics(&metrics, &[(0, success_metric)]);
        let health = observability.v2_connection_health().snapshot();
        assert_eq!(
            health.account_capacity_event_counts,
            [(success_metric.to_string(), 1)].into()
        );
        assert_eq!(
            health
                .account_capacity_event_worker_counts
                .iter()
                .map(|counts| (counts.worker_id, counts.event_counts.clone()))
                .collect::<Vec<_>>(),
            vec![(0, [(success_metric.to_string(), 1)].into())]
        );
        assert_eq!(
            health.last_account_capacity_event.as_deref(),
            Some(success_metric)
        );
        assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
        assert_eq!(
            health.last_account_capacity_event_tenant_id.as_deref(),
            Some("tenant-a")
        );
        assert_eq!(
            health.last_account_capacity_event_project_id.as_deref(),
            Some("project-a")
        );
        assert_eq!(
            health.last_account_capacity_event_reason.as_deref(),
            Some("thread id request restored on a replacement account-backed worker")
        );
        assert_eq!(health.last_account_capacity_event_at.is_some(), true);
        let handoff_event = operator_events_rx
            .recv()
            .await
            .unwrap_or_else(|_| panic!("{method} handoff success event should be published"));
        assert_eq!(
            handoff_event.method,
            "gateway/accountThreadHandoffSucceeded"
        );
        assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
        assert_eq!(
            handoff_event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": method,
                "threadId": "thread-worker-b",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "replacementWorkerId": 0,
                "replacementAccountId": "acct-a",
            })
        );

        router.shutdown().await.expect("router shutdown");
    }
}

#[tokio::test]
async fn additional_thread_id_controls_record_handoff_failure_when_no_replacement_restores_context()
{
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            "thread_name_set_handoff_failure",
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            "thread_memory_mode_set_handoff_failure",
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_decrement_elicitation_handoff_failure",
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            }),
            "thread_inject_items_handoff_failure",
        ),
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_unsubscribe_handoff_failure",
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_compact_start_handoff_failure",
        ),
    ];

    for (method, request_id, params, failure_metric) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: "thread not found: thread-worker-b".to_string(),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_idle_session().await;
        let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
            (worker_a.clone(), Some("acct-a".to_string())),
            (worker_b.clone(), Some("acct-b".to_string())),
        ]));
        worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-a".to_string(),
            project_id: Some("project-a".to_string()),
        };
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );

        let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
            vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
        )
        .with_worker_health(worker_health);
        let initialize_params = InitializeParams {
            client_info: ClientInfo {
                name: "codex-tui".to_string(),
                title: None,
                version: "0.0.0-test".to_string(),
            },
            capabilities: None,
        };
        let mut router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");
        let admission = GatewayAdmissionController::default();
        let metrics = in_memory_metrics();
        let (operator_events_tx, _) = broadcast::channel(4);
        let mut operator_events_rx = operator_events_tx.subscribe();
        let observability = GatewayObservability::new(Some(metrics.clone()), false)
            .with_operator_events(operator_events_tx);
        let connection = GatewayV2ConnectionContext {
            admission: &admission,
            observability: &observability,
            scope_registry: &scope_registry,
            request_context: &context,
            client_send_timeout: Duration::from_secs(10),
            max_pending_server_requests: 4,
            max_pending_client_requests: 4,
            opt_out_notification_methods: HashSet::new(),
        };

        let error = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(request_id.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .unwrap_err();

        let expected_message = format!(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}, and no replacement worker restored the context"
        );
        assert_eq!(error.to_string(), expected_message);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_v2_account_capacity_event_metrics(&metrics, &[(1, failure_metric)]);
        let health = observability.v2_connection_health().snapshot();
        assert_eq!(
            health.account_capacity_event_counts,
            [(failure_metric.to_string(), 1)].into()
        );
        assert_eq!(
            health
                .account_capacity_event_worker_counts
                .iter()
                .map(|counts| (counts.worker_id, counts.event_counts.clone()))
                .collect::<Vec<_>>(),
            vec![(1, [(failure_metric.to_string(), 1)].into())]
        );
        assert_eq!(
            health.last_account_capacity_event.as_deref(),
            Some(failure_metric)
        );
        assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
        assert_eq!(
            health.last_account_capacity_event_tenant_id.as_deref(),
            Some("tenant-a")
        );
        assert_eq!(
            health.last_account_capacity_event_project_id.as_deref(),
            Some("project-a")
        );
        assert_eq!(
            health.last_account_capacity_event_reason.as_deref(),
            Some(expected_message.as_str())
        );
        assert_eq!(health.last_account_capacity_event_at.is_some(), true);
        let handoff_event = operator_events_rx
            .recv()
            .await
            .unwrap_or_else(|_| panic!("{method} handoff failure event should be published"));
        assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
        assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
        assert_eq!(
            handoff_event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": method,
                "threadId": "thread-worker-b",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "reason": expected_message,
            })
        );

        router.shutdown().await.expect("router shutdown");
    }
}

#[tokio::test]
async fn thread_id_resume_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/resume",
        thread_resume_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(thread_resume_params),
                    trace: None,
                },
            )
            .await
            .expect_err("thread/resume should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("thread_resume_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_resume_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_id_fork_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_fork_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/fork",
        thread_fork_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-forked-worker-a",
                "name": "Worker A forked thread",
                "cwd": "/tmp/worker-a",
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/worker-a",
            "instructionSources": [],
            "approvalPolicy": "on-request",
            "approvalsReviewer": "user",
            "sandbox": { "type": "dangerFullAccess" },
            "reasoningEffort": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect("thread/fork should try a replacement worker")
            .expect("thread/fork should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-forked-worker-a")
    );
    assert_eq!(
        scope_registry.thread_worker_id("thread-forked-worker-a"),
        Some(0)
    );
    assert!(logs.contains("method=\"thread/fork\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_fork_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_fork_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("thread_fork_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_fork_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread fork handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/fork",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_id_fork_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_fork_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/fork",
        thread_fork_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect_err("thread/fork should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("thread_fork_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_fork_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_fork_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("thread_fork_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_fork_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread fork handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/fork",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn handle_client_request_routes_path_based_thread_resume_by_visible_path() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/worker-b/rollout.jsonl",
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/resume",
        thread_resume_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/worker-b/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
            },
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_path_with_worker(
        "/tmp/worker-b/rollout.jsonl",
        context.clone(),
        None,
    );

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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let result = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume-path".to_string()),
            method: "thread/resume".to_string(),
            params: Some(serde_json::json!({
                "threadId": "ignored-thread-id",
                "path": "/tmp/worker-b/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect("path-based thread/resume should reach downstream workers")
    .expect("path-based thread/resume should succeed through visible path discovery");

    assert_eq!(
        result,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-b/rollout.jsonl"),
        Some(1)
    );
}

#[tokio::test]
async fn path_based_thread_resume_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
                "path": "/tmp/shared/rollout.jsonl",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_c = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Worker C restored thread",
                "cwd": "/tmp/worker-c",
                "path": "/tmp/shared/rollout.jsonl",
            },
        }),
    )
    .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
        (worker_c.clone(), Some("acct-c".to_string())),
    ]));
    worker_health.mark_unhealthy(0, Some("socket closed".to_string()));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_c,
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
        vec![
            Some("acct-a".to_string()),
            Some("acct-b".to_string()),
            Some("acct-c".to_string()),
        ],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume-path".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "ignored-thread-id",
                        "path": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect("path-based thread/resume should try a replacement worker")
            .expect("path-based thread/resume should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-worker-a")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(2));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(2)
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=2"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(2, "path_thread_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(2, [("path_thread_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(2));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("path-based thread request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff success event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffSucceeded");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-c",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_resume_rejects_replacement_worker_returning_wrong_path() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong restored path",
                "cwd": "/tmp/worker-a",
                "path": "/tmp/other/rollout.jsonl",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume-path".to_string()),
            method: "thread/resume".to_string(),
            params: Some(serde_json::json!({
                "threadId": "ignored-thread-id",
                "path": "/tmp/shared/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect_err("path-based thread/resume should fail closed on wrong restored path");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), None);
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(1)
    );
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/other/rollout.jsonl"),
        None
    );
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("path_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_resume_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/resume",
        thread_resume_params,
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/shared/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume-path".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "ignored-thread-id",
                        "path": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect_err("path-based thread/resume should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("path_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_fork_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_fork_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/fork",
        thread_fork_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-forked-worker-a",
                "name": "Worker A forked thread",
                "cwd": "/tmp/worker-a",
                "path": "/tmp/shared/rollout.jsonl",
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/worker-a",
            "instructionSources": [],
            "approvalPolicy": "on-request",
            "approvalsReviewer": "user",
            "sandbox": { "type": "dangerFullAccess" },
            "reasoningEffort": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork-path".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect("path-based thread/fork should try a replacement worker")
            .expect("path-based thread/fork should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        super::super::super::response_thread_id(&response),
        Some("thread-forked-worker-a")
    );
    assert_eq!(
        scope_registry.thread_worker_id("thread-forked-worker-a"),
        Some(0)
    );
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"thread/fork\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "path_thread_handoff_success")]);
    assert_v2_account_capacity_event_health(
        &observability,
        0,
        "path_thread_handoff_success",
        1,
        "tenant-a",
        Some("project-a"),
        "path-based thread request restored on a replacement account-backed worker",
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_fork_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_fork_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/fork",
        thread_fork_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/shared/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork-path".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect_err("path-based thread/fork should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("method=\"thread/fork\""), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context",
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn handle_client_request_routes_conversation_summary_by_visible_rollout_path() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/worker-b/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/worker-b/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params,
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
                "preview": "Worker B summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-b",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_path_with_worker(
        "/tmp/worker-b/rollout.jsonl",
        context.clone(),
        None,
    );

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
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let result = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-path".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(serde_json::json!({
                "rolloutPath": "/tmp/worker-b/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect("getConversationSummary should reach downstream workers")
    .expect("getConversationSummary should succeed through visible path discovery");

    assert_eq!(
        result["summary"]["conversationId"],
        serde_json::json!("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-b/rollout.jsonl"),
        Some(1)
    );
}

#[tokio::test]
async fn conversation_summary_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-a",
                "path": "/tmp/shared/rollout.jsonl",
                "preview": "Worker A summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("conversation-summary-path".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(summary_params),
                    trace: None,
                },
            )
            .await
            .expect("getConversationSummary should try a replacement worker")
            .expect("getConversationSummary should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        response["summary"]["conversationId"],
        serde_json::json!("thread-worker-a")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "path_thread_handoff_success")]);
    assert_v2_account_capacity_event_health(
        &observability,
        0,
        "path_thread_handoff_success",
        1,
        "tenant-a",
        Some("project-a"),
        "path-based thread request restored on a replacement account-backed worker",
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_rejects_replacement_worker_returning_wrong_path() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-a",
                "path": "/tmp/other/rollout.jsonl",
                "preview": "Wrong worker summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-path".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary should fail closed on wrong restored path");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), None);
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(1)
    );
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/other/rollout.jsonl"),
        None
    );
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_records_handoff_failure_when_no_replacement_restores_context() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params,
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/shared/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("conversation-summary-path".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(serde_json::json!({
                        "rolloutPath": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect_err("getConversationSummary should fail closed without a replacement worker"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_by_id_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": thread_id,
                "path": "/tmp/worker-a/rollout.jsonl",
                "preview": "Worker A summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            super::super::super::handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("conversation-summary-id".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(summary_params),
                    trace: None,
                },
            )
            .await
            .expect("getConversationSummary by id should try a replacement worker")
            .expect("getConversationSummary by id should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        response["summary"]["conversationId"],
        serde_json::json!(thread_id)
    );
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(0));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-a/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "conversation_summary_handoff_success")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            0,
            [("conversation_summary_handoff_success".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_by_id_rejects_replacement_worker_returning_wrong_thread() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let wrong_thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c9";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": wrong_thread_id,
                "path": "/tmp/worker-a/rollout.jsonl",
                "preview": "Wrong worker summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-id".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary by id should fail closed on wrong conversation id");

    let expected_message = format!(
        "thread {thread_id} is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(error.to_string(), expected_message);
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(1));
    assert_eq!(scope_registry.thread_worker_id(wrong_thread_id), None);
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-a/rollout.jsonl"),
        None
    );
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "conversation_summary_handoff_failure")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [("conversation_summary_handoff_failure".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(expected_message.as_str())
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": expected_message,
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_by_id_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params.clone(),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: format!("thread not found: {thread_id}"),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health);
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
            .await
            .expect("downstream router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let error = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-id".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary by id should fail closed");

    let expected_message = format!(
        "thread {thread_id} is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(error.to_string(), expected_message);
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(1));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "conversation_summary_handoff_failure")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [("conversation_summary_handoff_failure".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(expected_message.as_str())
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": expected_message,
        })
    );

    router.shutdown().await.expect("router shutdown");
}
