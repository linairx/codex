use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_3_reconnect.rs"]
mod v2_tests_cases_3_reconnect;

#[path = "v2_tests_cases_3_reconnect_and_fallback.rs"]
mod v2_tests_cases_3_reconnect_and_fallback;

#[path = "v2_tests_cases_3_primary_worker.rs"]
mod v2_tests_cases_3_primary_worker;

#[path = "v2_tests_cases_3_discovery_backoff.rs"]
mod v2_tests_cases_3_discovery_backoff;

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_thread_list() {
    let worker_a = start_mock_remote_server_for_thread_list_and_read(
        "thread-worker-a",
        "Worker A thread",
        "/tmp/worker-a",
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
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
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
    assert_eq!(router.worker_count(), 2);
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker before reconnect"
    );
    assert_eq!(router.worker_count(), 1);

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
            id: RequestId::String("thread-list".to_string()),
            method: "thread/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread/list should reach downstream workers")
    .expect("thread/list should succeed after reconnecting the missing worker");

    let listed: ThreadListResponse =
        serde_json::from_value(result).expect("thread list response should decode");
    assert_eq!(listed.next_cursor, None);
    assert_eq!(listed.backwards_cursor, None);
    assert_eq!(
        listed
            .data
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-worker-b", "thread-worker-a"]
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_loaded_thread_list() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "thread/loaded/list",
        serde_json::json!({
            "data": ["thread-worker-a"],
            "nextCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "thread/loaded/list",
        serde_json::json!({
            "data": ["thread-worker-b"],
            "nextCursor": null,
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
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
    assert_eq!(router.worker_count(), 2);
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker before reconnect"
    );
    assert_eq!(router.worker_count(), 1);

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
    .expect("thread/loaded/list should reach downstream workers")
    .expect("thread/loaded/list should succeed after reconnecting the missing worker");

    let loaded: ThreadLoadedListResponse =
        serde_json::from_value(result).expect("loaded thread list response should decode");
    assert_eq!(router.worker_count(), 2);
    assert_eq!(loaded.next_cursor, None);
    assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_fanout_setup_mutations() {
    let cases = vec![
        (
            "externalAgentConfig/import",
            serde_json::json!({
                "migrationItems": [],
            }),
            serde_json::json!({}),
        ),
        (
            "config/batchWrite",
            serde_json::json!({
                "edits": [],
                "filePath": "/tmp/shared/config.toml",
                "expectedVersion": null,
                "reloadUserConfig": true,
            }),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        (
            "config/value/write",
            serde_json::json!({
                "keyPath": "plugins.shared-plugin",
                "value": {
                    "enabled": true,
                },
                "mergeStrategy": "upsert",
                "filePath": null,
                "expectedVersion": null,
            }),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        (
            "marketplace/add",
            serde_json::json!({
                "source": "https://example.com/gateway-marketplace.git",
                "refName": "main",
                "sparsePaths": ["plugins/shared-plugin"],
            }),
            serde_json::json!({
                "marketplaceName": "gateway-marketplace",
                "installedRoot": "/tmp/shared/marketplace",
                "alreadyAdded": false,
            }),
        ),
        (
            "skills/config/write",
            serde_json::json!({
                "path": "/tmp/shared/skills/shared-skill",
                "name": null,
                "enabled": true,
            }),
            serde_json::json!({
                "effectiveEnabled": true,
            }),
        ),
        (
            "experimentalFeature/enablement/set",
            serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            }),
            serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            }),
        ),
        (
            "config/mcpServer/reload",
            serde_json::Value::Null,
            serde_json::json!({}),
        ),
        (
            "memory/reset",
            serde_json::Value::Null,
            serde_json::json!({}),
        ),
        (
            "account/logout",
            serde_json::Value::Null,
            serde_json::json!({}),
        ),
    ];

    for (method, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
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
        assert_eq!(router.worker_count(), 2);
        assert!(
            router.remove_worker(Some(1)),
            "test should drop the second worker before reconnect"
        );
        assert_eq!(router.worker_count(), 1);

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
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params: (params != serde_json::Value::Null).then_some(params),
                trace: None,
            },
        )
        .await
        .expect("setup mutation request should reach downstream workers")
        .expect("setup mutation request should succeed after reconnecting the missing worker");

        assert_eq!(router.worker_count(), 2);
        assert_eq!(result, expected_result);
        assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);
    }
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_fs_watch_and_unwatch() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
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
    assert_eq!(router.worker_count(), 2);
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker before reconnect"
    );
    assert_eq!(router.worker_count(), 1);

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

    let watch_result = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("fs-watch".to_string()),
            method: "fs/watch".to_string(),
            params: Some(serde_json::json!({
                "watchId": "watch-shared",
                "path": "/tmp/shared/project/.git/HEAD",
            })),
            trace: None,
        },
    )
    .await
    .expect("fs/watch should reach downstream workers")
    .expect("fs/watch should succeed after reconnecting the missing worker");
    let watch_response: FsWatchResponse =
        serde_json::from_value(watch_result).expect("fs/watch response should decode");
    assert_eq!(
        watch_response.path.as_ref().to_string_lossy(),
        "/tmp/shared/project/.git/HEAD"
    );
    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        *worker_a_requests.lock().await,
        vec!["fs/watch".to_string()]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["fs/watch".to_string()]
    );

    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker again before reconnecting unwatch"
    );
    assert_eq!(router.worker_count(), 1);

    let unwatch_result = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("fs-unwatch".to_string()),
            method: "fs/unwatch".to_string(),
            params: Some(serde_json::json!({
                "watchId": "watch-shared",
            })),
            trace: None,
        },
    )
    .await
    .expect("fs/unwatch should reach downstream workers")
    .expect("fs/unwatch should succeed after reconnecting the missing worker");
    let _: FsUnwatchResponse =
        serde_json::from_value(unwatch_result).expect("fs/unwatch response should decode");
    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        *worker_a_requests.lock().await,
        vec!["fs/watch".to_string(), "fs/unwatch".to_string()]
    );
    assert_eq!(
        *worker_b_requests.lock().await,
        vec![
            "fs/watch".to_string(),
            "fs/watch".to_string(),
            "fs/unwatch".to_string(),
        ]
    );
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_fuzzy_file_search_during_reconnect_backoff() {
    let cases = vec![
        (
            "fuzzyFileSearch",
            Some(serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-backoff",
            })),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/project",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 42,
                    "indices": [5, 6, 7, 8],
                }],
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            Some(serde_json::json!({
                "sessionId": "search-session-backoff",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            Some(serde_json::json!({
                "sessionId": "search-session-backoff",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            Some(serde_json::json!({
                "sessionId": "search-session-backoff",
            })),
            serde_json::json!({}),
        ),
    ];

    for (method, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result,
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
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
            router.remove_worker(Some(0)),
            "test should drop the primary worker before applying reconnect backoff"
        );
        router.record_worker_reconnect_failure(0, Instant::now(), Duration::from_secs(60));

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
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params,
                trace: None,
            },
        )
        .await
        .expect_err("fuzzy file search should not fall back during reconnect backoff");

        if method == "fuzzyFileSearch" {
            assert_eq!(
                err.to_string(),
                "required worker routes are unavailable for fuzzyFileSearch: [0]"
            );
        } else {
            assert_eq!(
                err.to_string(),
                format!("primary worker route is unavailable for {method}")
            );
        }
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
        assert_v2_fail_closed_request_metric(&metrics, method, true);
    }
}

#[tokio::test]
async fn handle_client_request_does_not_apply_fanout_setup_mutations_during_reconnect_backoff() {
    let cases = vec![
        (
            "externalAgentConfig/import",
            Some(serde_json::json!({
                "migrationItems": [],
            })),
            serde_json::json!({}),
        ),
        (
            "config/batchWrite",
            Some(serde_json::json!({
                "edits": [],
                "filePath": "/tmp/shared/config.toml",
                "expectedVersion": null,
                "reloadUserConfig": true,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        (
            "config/value/write",
            Some(serde_json::json!({
                "keyPath": "plugins.shared-plugin",
                "value": {
                    "enabled": true,
                },
                "mergeStrategy": "upsert",
                "filePath": null,
                "expectedVersion": null,
            })),
            serde_json::json!({
                "status": "ok",
                "version": "worker-a",
                "filePath": "/tmp/shared/config.toml",
                "overriddenMetadata": null,
            }),
        ),
        (
            "marketplace/add",
            Some(serde_json::json!({
                "source": "https://example.com/gateway-marketplace.git",
                "refName": "main",
                "sparsePaths": ["plugins/shared-plugin"],
            })),
            serde_json::json!({
                "marketplaceName": "gateway-marketplace",
                "installedRoot": "/tmp/shared/marketplace",
                "alreadyAdded": false,
            }),
        ),
        (
            "skills/config/write",
            Some(serde_json::json!({
                "path": "/tmp/shared/skills/shared-skill",
                "name": null,
                "enabled": true,
            })),
            serde_json::json!({
                "effectiveEnabled": true,
            }),
        ),
        (
            "experimentalFeature/enablement/set",
            Some(serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            })),
            serde_json::json!({
                "enablement": {
                    "gateway-test-feature": true,
                },
            }),
        ),
        ("config/mcpServer/reload", None, serde_json::json!({})),
        ("memory/reset", None, serde_json::json!({})),
        ("account/logout", None, serde_json::json!({})),
        (
            "account/login/start",
            Some(serde_json::json!({
                "type": "apiKey",
                "apiKey": "sk-test",
            })),
            serde_json::json!({
                "type": "apiKey",
                "accountId": "acct-worker-a",
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
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
            "test should drop the second worker before applying reconnect backoff"
        );
        router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

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

        let err = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params: params.clone(),
                trace: None,
            },
        )
        .await
        .expect_err("fanout setup mutation should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            format!("required worker routes are unavailable for {method}: [1]")
        );
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    }
}

#[tokio::test]
async fn handle_client_request_does_not_apply_fs_watch_during_reconnect_backoff() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_fs_watch_and_unwatch().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
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
        "test should drop the second worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

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

    let err = super::super::super::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("fs-watch".to_string()),
            method: "fs/watch".to_string(),
            params: Some(serde_json::json!({
                "watchId": "watch-shared",
                "path": "/tmp/shared/project/.git/HEAD",
            })),
            trace: None,
        },
    )
    .await
    .expect_err("fs/watch should fail closed during reconnect backoff");

    assert_eq!(
        err.to_string(),
        "required worker routes are unavailable for fs/watch: [1]"
    );
    assert_eq!(router.worker_count(), 1);
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_config_read_by_cwd_during_reconnect_backoff() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-config".to_string(),
        project_id: Some("project-config".to_string()),
    };
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
        "test should drop the matching worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

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

    let logs = capture_logs_async(async {
        let err = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(serde_json::json!({
                    "includeLayers": true,
                    "cwd": "/tmp/worker-b/subdir",
                })),
                trace: None,
            },
        )
        .await
        .expect_err("cwd-aware config/read should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "required worker routes are unavailable for config/read: [1]"
        );
        assert_eq!(router.worker_count(), 1);
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"config/read\""));
    assert!(logs.contains("tenant-config"));
    assert!(logs.contains("project-config"));
    assert!(logs.contains("available_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "available_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("unavailable_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "unavailable_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_routes=[(1, \"{worker_b_url}\", "
    )));
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_threadless_config_read_during_reconnect_backoff() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-b/config.toml",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-config".to_string(),
        project_id: Some("project-config".to_string()),
    };
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
        router.remove_worker(Some(0)),
        "test should drop the primary worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(0, Instant::now(), Duration::from_secs(60));

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

    let logs = capture_logs_async(async {
        let err = super::super::super::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(serde_json::json!({
                    "includeLayers": true,
                    "cwd": null,
                })),
                trace: None,
            },
        )
        .await
        .expect_err("threadless config/read should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "primary worker route is unavailable for config/read"
        );
        assert_eq!(router.worker_count(), 1);
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"config/read\""));
    assert!(logs.contains("tenant-config"));
    assert!(logs.contains("project-config"));
    assert!(logs.contains("available_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "available_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("unavailable_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "unavailable_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_routes=[(0, \"{worker_a_url}\", "
    )));
}

#[path = "v2_tests_cases_3_late.rs"]
mod v2_tests_cases_3_late;

#[path = "v2_tests_cases_3_reconnect_backoff.rs"]
mod v2_tests_cases_3_reconnect_backoff;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_3_late::*;
