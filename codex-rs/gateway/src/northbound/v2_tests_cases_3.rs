use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_realtime_list_voices() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "thread/realtime/listVoices",
        serde_json::json!({
            "voices": {
                "v1": ["juniper", "maple"],
                "v2": ["alloy"],
                "defaultV1": "juniper",
                "defaultV2": "alloy",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "thread/realtime/listVoices",
        serde_json::json!({
            "voices": {
                "v1": ["maple", "cove"],
                "v2": ["alloy", "marin"],
                "defaultV1": "cove",
                "defaultV2": "marin",
            },
        }),
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
            id: RequestId::String("realtime-list-voices".to_string()),
            method: "thread/realtime/listVoices".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("thread/realtime/listVoices should reach downstream workers")
    .expect("thread/realtime/listVoices should succeed after reconnecting the missing worker");

    let response: ThreadRealtimeListVoicesResponse =
        serde_json::from_value(result).expect("thread/realtime/listVoices response should decode");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        response,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList {
                v1: vec![
                    RealtimeVoice::Juniper,
                    RealtimeVoice::Maple,
                    RealtimeVoice::Cove,
                ],
                v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
                default_v1: RealtimeVoice::Juniper,
                default_v2: RealtimeVoice::Alloy,
            },
        }
    );
}

#[tokio::test]
async fn handle_client_request_routes_multi_worker_config_read_by_matching_cwd() {
    let (worker_a_tx, worker_a_rx) = oneshot::channel();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_tx,
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
    let (worker_b_tx, worker_b_rx) = oneshot::channel();
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_tx,
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
    .expect("config/read should reach downstream workers")
    .expect("config/read should succeed through matching worker");

    assert_eq!(
        result,
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
        })
    );

    let worker_a_request = worker_a_rx.await.expect("worker A should capture request");
    let worker_b_request = worker_b_rx.await.expect("worker B should capture request");
    assert_eq!(worker_a_request.method, "config/read");
    assert_eq!(worker_b_request.method, "config/read");
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_config_read_by_matching_cwd() {
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
    .expect("config/read should reach downstream workers")
    .expect("config/read should succeed after reconnecting the missing worker");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        result,
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
        })
    );
}

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
async fn handle_client_request_reconnects_missing_worker_before_plugin_management_fallback_requests()
 {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
        (
            "gitDiffToRemote",
            serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            }),
            serde_json::json!({
                "sha": "0123456789abcdef0123456789abcdef01234567",
                "diff": "diff --git a/README.md b/README.md\n",
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b =
            start_mock_remote_server_for_reconnectable_request(method, expected_result.clone())
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
                params: Some(params),
                trace: None,
            },
        )
        .await
        .expect("plugin management request should reach downstream workers")
        .expect("plugin management request should succeed after reconnecting the missing worker");

        assert_eq!(router.worker_count(), 2);
        assert_eq!(result, expected_result);
    }
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_mcp_server_oauth_login() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "mcpServer/oauth/login",
        serde_json::json!({
            "name": "shared-mcp",
        }),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "mcpServer/oauth/login missing on worker-a".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "mcpServer/oauth/login",
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/shared-mcp",
        }),
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
            id: RequestId::String("mcp-oauth-login-request".to_string()),
            method: "mcpServer/oauth/login".to_string(),
            params: Some(serde_json::json!({
                "name": "shared-mcp",
            })),
            trace: None,
        },
    )
    .await
    .expect("mcpServer/oauth/login should reach downstream workers")
    .expect("mcpServer/oauth/login should succeed after reconnecting the missing worker");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        result,
        serde_json::json!({
            "authorizationUrl": "https://example.test/oauth/shared-mcp",
        })
    );
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
async fn handle_client_request_reconnects_missing_primary_worker_before_primary_worker_requests() {
    let cases = vec![
        (
            "configRequirements/read",
            None,
            serde_json::json!({
                "requirements": null,
            }),
        ),
        (
            "account/login/start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-reconnected",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            Some(serde_json::json!({
                "loginId": "login-reconnected",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "command/exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                "processId": "proc-reconnected",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "feedback/upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway reconnect regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-reconnected",
            }),
        ),
        (
            "account/sendAddCreditsNudgeEmail",
            Some(serde_json::json!({
                "creditType": "credits",
            })),
            serde_json::json!({
                "status": "sent",
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
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
        if method == "feedback/upload" {
            scope_registry.register_thread("thread-visible".to_string(), context.clone());
        }
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
            router.remove_worker(Some(0)),
            "test should drop the primary worker before reconnect"
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
                params: params.clone(),
                trace: None,
            },
        )
        .await
        .expect("primary-worker request should reach downstream workers")
        .expect("primary-worker request should succeed after reconnecting the primary worker");

        assert_eq!(router.worker_count(), 2);
        assert_eq!(result, expected_result);
        let worker_a_requests = worker_a_requests.lock().await.clone();
        if method == "configRequirements/read" {
            assert!(worker_a_requests.is_empty());
        } else {
            assert_eq!(worker_a_requests, vec![method.to_string()]);
        }
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    }
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_primary_worker_requests_during_reconnect_backoff()
{
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "command/exec",
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        )
        .await;
    let worker_a_url = worker_a.clone();
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "command/exec",
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        )
        .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-primary".to_string(),
        project_id: Some("project-primary".to_string()),
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
                id: RequestId::String("command-exec-request".to_string()),
                method: "command/exec".to_string(),
                params: Some(serde_json::json!({
                    "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                    "processId": "proc-reconnected",
                    "tty": true,
                    "streamStdin": true,
                    "streamStdoutStderr": true,
                    "outputBytesCap": null,
                    "timeoutMs": null,
                    "cwd": null,
                    "env": null,
                    "size": {
                        "rows": 24,
                        "cols": 80,
                    },
                    "sandboxPolicy": null,
                })),
                trace: None,
            },
        )
        .await
        .expect_err("primary-worker request should not fall back during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "primary worker route is unavailable for command/exec"
        );
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("tenant-primary"));
    assert!(logs.contains("project-primary"));
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

#[tokio::test]
async fn handle_client_request_does_not_fallback_primary_worker_method_family_during_reconnect_backoff()
 {
    let cases = vec![
        (
            "configRequirements/read",
            None,
            serde_json::json!({
                "requirements": [],
                "validationErrors": [],
            }),
        ),
        (
            "account/login/start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-primary",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            Some(serde_json::json!({
                "loginId": "login-primary",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "account/sendAddCreditsNudgeEmail",
            Some(serde_json::json!({
                "creditType": "credits",
            })),
            serde_json::json!({
                "status": "sent",
            }),
        ),
        (
            "feedback/upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway primary-worker backoff regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-primary",
            }),
        ),
        (
            "command/exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-primary"],
                "processId": "proc-primary",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            Some(serde_json::json!({
                "processId": "proc-primary",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            Some(serde_json::json!({
                "processId": "proc-primary",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            Some(serde_json::json!({
                "processId": "proc-primary",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/readFile",
            Some(serde_json::json!({
                "path": "/tmp/project/input.txt",
            })),
            serde_json::json!({
                "dataBase64": "Z2F0ZXdheS1maWxl",
            }),
        ),
        (
            "fs/writeFile",
            Some(serde_json::json!({
                "path": "/tmp/project/output.txt",
                "dataBase64": "Z2F0ZXdheS13cml0ZQ==",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/createDirectory",
            Some(serde_json::json!({
                "path": "/tmp/project/nested",
                "recursive": true,
            })),
            serde_json::json!({}),
        ),
        (
            "fs/getMetadata",
            Some(serde_json::json!({
                "path": "/tmp/project/output.txt",
            })),
            serde_json::json!({
                "isDirectory": false,
                "isFile": true,
                "isSymlink": false,
                "createdAtMs": 0,
                "modifiedAtMs": 0,
            }),
        ),
        (
            "fs/readDirectory",
            Some(serde_json::json!({
                "path": "/tmp/project",
            })),
            serde_json::json!({
                "entries": [{
                    "fileName": "output.txt",
                    "isDirectory": false,
                    "isFile": true,
                }],
            }),
        ),
        (
            "fs/copy",
            Some(serde_json::json!({
                "sourcePath": "/tmp/project/output.txt",
                "destinationPath": "/tmp/project/copy.txt",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/remove",
            Some(serde_json::json!({
                "path": "/tmp/project/copy.txt",
                "recursive": true,
                "force": true,
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
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
                expected_result,
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        if method == "feedback/upload" {
            scope_registry.register_thread("thread-visible".to_string(), context.clone());
        }
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
        .expect_err("primary-worker method should not fall back during reconnect backoff");

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
async fn aggregated_discovery_requests_fail_closed_during_reconnect_backoff() {
    let cases = vec![
        (
            "account/read",
            Some(serde_json::json!({
                "refreshToken": false,
            })),
            serde_json::json!({
                "account": null,
                "requiresOpenaiAuth": false,
            }),
        ),
        (
            "getAuthStatus",
            Some(serde_json::json!({
                "includeToken": false,
                "refreshToken": false,
            })),
            serde_json::json!({
                "authMethod": null,
                "authToken": null,
                "requiresOpenaiAuth": false,
            }),
        ),
        (
            "account/rateLimits/read",
            None,
            serde_json::json!({
                "rateLimits": {
                    "limitId": null,
                    "limitName": null,
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": null,
            }),
        ),
        (
            "app/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "threadId": null,
                "forceRefetch": false,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "mcpServerStatus/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "detail": "toolsAndAuthOnly",
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "externalAgentConfig/detect",
            Some(serde_json::json!({})),
            serde_json::json!({
                "items": [],
            }),
        ),
        (
            "model/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": null,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "skills/list",
            Some(serde_json::json!({})),
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "experimentalFeature/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "collaborationMode/list",
            Some(serde_json::json!({})),
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "plugin/list",
            Some(serde_json::json!({
                "cwds": null,
            })),
            serde_json::json!({
                "marketplaces": [],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [],
            }),
        ),
        (
            "thread/realtime/listVoices",
            Some(serde_json::json!({})),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper"],
                    "v2": ["alloy"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
        ),
    ];

    for (method, params, response) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                response.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                response.clone(),
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
            "test should drop a worker before applying reconnect backoff"
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
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params,
                trace: None,
            },
        )
        .await
        .expect_err("aggregated discovery should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            format!("required worker routes are unavailable for {method}: [1]")
        );
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
        assert_v2_fail_closed_request_metric(&metrics, method, true);
    }
}

#[tokio::test]
async fn degraded_thread_discovery_logs_unavailable_worker_routes() {
    let cases = vec![
        (
            "thread/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
    ];

    for (method, params, response) in cases {
        let logs = capture_logs_async(async {
            let (worker_a, worker_a_requests) =
                start_mock_remote_server_for_reconnectable_request_with_recording(
                    method,
                    response.clone(),
                )
                .await;
            let (worker_b, worker_b_requests) =
                start_mock_remote_server_for_reconnectable_request_with_recording(
                    method,
                    response.clone(),
                )
                .await;
            let scope_registry = Arc::new(GatewayScopeRegistry::default());
            let context = GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
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
                "test should drop a worker before applying reconnect backoff"
            );
            router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

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

            let result = super::super::super::handle_client_request(
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
            .expect("degraded thread discovery should reach available workers")
            .expect("degraded thread discovery should succeed");

            assert!(result.is_object());
            assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
            assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
            assert_v2_degraded_thread_discovery_metric(&metrics, method, true);
        })
        .await;

        assert!(
            logs.contains("serving degraded multi-worker thread discovery from available workers")
        );
        assert_eq!(
            logs.matches("serving degraded multi-worker thread discovery from available workers")
                .count(),
            1
        );
        assert!(logs.contains("tenant-visible"));
        assert!(logs.contains("project-visible"));
        assert!(logs.contains(&format!("method=\"{method}\"")));
        assert!(logs.contains("available_worker_ids=[0]"));
        assert!(logs.contains("available_worker_websocket_urls=["));
        assert!(logs.contains("unavailable_worker_ids=[1]"));
        assert!(logs.contains("unavailable_worker_websocket_urls=["));
        assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
        assert!(logs.contains("reconnect_backoff_worker_websocket_urls=["));
        assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
        assert!(logs.contains("reconnect_backoff_worker_routes=["));
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

#[tokio::test]
async fn handle_client_request_does_not_fallback_plugin_management_during_reconnect_backoff() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
        (
            "gitDiffToRemote",
            serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            }),
            serde_json::json!({
                "sha": "0123456789abcdef0123456789abcdef01234567",
                "diff": "diff --git a/README.md b/README.md\n",
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
            "test should drop the fallback worker before applying reconnect backoff"
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
                params: Some(params),
                trace: None,
            },
        )
        .await
        .expect_err("plugin management fallback should fail closed during reconnect backoff");

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
async fn handle_client_request_does_not_fallback_mcp_oauth_login_during_reconnect_backoff() {
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "mcpServer/oauth/login",
            serde_json::json!({
                "authorizationUrl": "https://example.test/oauth/shared-mcp",
            }),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "mcpServer/oauth/login",
            serde_json::json!({
                "authorizationUrl": "https://example.test/oauth/shared-mcp",
            }),
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
        "test should drop the fallback worker before applying reconnect backoff"
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
            id: RequestId::String("mcp-oauth-login-request".to_string()),
            method: "mcpServer/oauth/login".to_string(),
            params: Some(serde_json::json!({
                "name": "shared-mcp",
            })),
            trace: None,
        },
    )
    .await
    .expect_err("mcpServer/oauth/login should fail closed during reconnect backoff");

    assert_eq!(
        err.to_string(),
        "required worker routes are unavailable for mcpServer/oauth/login: [1]"
    );
    assert_eq!(router.worker_count(), 1);
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    assert_v2_fail_closed_request_metric(&metrics, "mcpServer/oauth/login", true);
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_connection_notifications() {
    let cases = vec![
        (
            "account/updated",
            serde_json::json!({
                "authMode": null,
                "planType": null,
            }),
        ),
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "primary",
                    "limitName": "Primary",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, notification_params) in cases {
        let worker_a = start_mock_remote_server_for_connection_notification(
            method,
            notification_params.clone(),
        )
        .await;
        let worker_b = start_mock_remote_server_for_connection_notification(
            method,
            notification_params.clone(),
        )
        .await;
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
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            notification_params,
        );

        let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(duplicate.is_err(), true);
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_mcp_startup_notifications() {
    let notification_params = serde_json::json!({
        "name": "gateway-mcp",
        "status": "ready",
        "error": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "mcpServer/startupStatus/updated",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "mcpServer/startupStatus/updated",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/startupStatus/updated",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(duplicate.is_err());
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/startupStatus/updated",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_login_completed_notifications()
{
    let notification_params = serde_json::json!({
        "loginId": "login-shared",
        "success": true,
        "error": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "account/login/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "account/login/completed",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/login/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "account/login/completed", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_mcp_oauth_login_completed_notifications()
 {
    let notification_params = serde_json::json!({
        "name": "shared-mcp",
        "success": true,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "mcpServer/oauthLogin/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "mcpServer/oauthLogin/completed",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/oauthLogin/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/oauthLogin/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_warning_notifications() {
    let notification_params = serde_json::json!({
        "message": "shared worker warning",
        "threadId": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "warning",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("warning", notification_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let worker_a_url = worker_a.clone();
    let worker_b_url = worker_b.clone();
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let logs = capture_logs_async(async {
        let mut request = format!("ws://{addr}/")
            .into_client_request()
            .expect("request should build");
        request.headers_mut().insert(
            "x-codex-tenant-id",
            "tenant-a".parse().expect("tenant header"),
        );
        request.headers_mut().insert(
            "x-codex-project-id",
            "project-a".parse().expect("project header"),
        );
        let (mut websocket, _response) = connect_async(request)
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            "warning",
            notification_params,
        );

        let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
        assert_eq!(duplicate.is_err(), true);
    })
    .await;
    assert_v2_suppressed_notification_metric(&metrics, "warning", "duplicate");
    assert!(logs.contains("suppressing exact-duplicate multi-worker connection notification"));
    assert!(logs.contains("tenant-a"), "{logs}");
    assert!(logs.contains("project-a"), "{logs}");
    assert!(logs.contains("worker_id=Some(1)"), "{logs}");
    assert!(logs.contains("original_worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains(&format!("worker_websocket_url=\"{worker_b_url}\"")),
        "{logs}"
    );
    assert!(
        logs.contains(&format!("original_worker_websocket_url=\"{worker_a_url}\"")),
        "{logs}"
    );
    assert!(logs.contains("method=\"warning\""), "{logs}");
    assert!(logs.contains("shared worker warning"), "{logs}");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_mcp_oauth_notifications() {
    let first_params = serde_json::json!({
        "name": "shared-mcp",
        "success": true,
    });
    let second_params = serde_json::json!({
        "name": "other-mcp",
        "success": true,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("mcpServer/oauthLogin/completed", first_params.clone()),
        ("mcpServer/oauthLogin/completed", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("mcpServer/oauthLogin/completed", first_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/oauthLogin/completed",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/oauthLogin/completed",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "mcpServer/oauthLogin/completed"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate mcpServer/oauthLogin/completed notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/oauthLogin/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_mcp_startup_notifications() {
    let first_params = serde_json::json!({
        "name": "shared-mcp",
        "status": "ready",
        "error": null,
    });
    let second_params = serde_json::json!({
        "name": "other-mcp",
        "status": "failed",
        "error": "startup failed",
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("mcpServer/startupStatus/updated", first_params.clone()),
        ("mcpServer/startupStatus/updated", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("mcpServer/startupStatus/updated", first_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/startupStatus/updated",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "mcpServer/startupStatus/updated",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "mcpServer/startupStatus/updated"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate mcpServer/startupStatus/updated notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(
        &metrics,
        "mcpServer/startupStatus/updated",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_account_updates() {
    let first_params = serde_json::json!({
        "authMode": null,
        "planType": null,
    });
    let second_params = serde_json::json!({
        "authMode": "apikey",
        "planType": null,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("account/updated", first_params.clone()),
        ("account/updated", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("account/updated", first_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/updated",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/updated",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "account/updated"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate account/updated notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "account/updated", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_login_completed_notifications() {
    let first_params = serde_json::json!({
        "loginId": null,
        "success": true,
        "error": null,
    });
    let second_params = serde_json::json!({
        "loginId": "other-login",
        "success": false,
        "error": "cancelled",
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("account/login/completed", first_params.clone()),
        ("account/login/completed", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("account/login/completed", first_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/login/completed",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "account/login/completed",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "account/login/completed"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate account/login/completed notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "account/login/completed", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_discovery_state_notifications() {
    let cases = vec![
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "shared",
                    "limitName": "Shared",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "other",
                    "limitName": "Other",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [{
                    "id": "shared-app",
                    "name": "Shared App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
            serde_json::json!({
                "data": [{
                    "id": "other-app",
                    "name": "Other App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
        )
        .await;
        let metrics = in_memory_metrics();
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_windows_setup_notifications() {
    let cases = vec![
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/shared-world-writable"],
                "extraCount": 1,
                "failedScan": false,
            }),
            serde_json::json!({
                "samplePaths": ["/tmp/other-world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "mode": "elevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
        )
        .await;
        let metrics = in_memory_metrics();
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_warning_notifications() {
    let first_params = serde_json::json!({
        "message": "first shared worker warning",
        "threadId": null,
    });
    let second_params = serde_json::json!({
        "message": "second shared worker warning",
        "threadId": null,
    });
    let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
        ("warning", first_params.clone()),
        ("warning", second_params.clone()),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
        vec![("warning", first_params.clone())],
        Duration::from_millis(50),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "warning",
        first_params.clone(),
    );
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "warning",
        second_params,
    );

    let deadline = Instant::now() + Duration::from_millis(200);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await else {
            break;
        };
        let message =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
        if let JSONRPCMessage::Notification(notification) = message {
            assert!(
                notification.method != "warning"
                    || notification.params.as_ref() != Some(&first_params),
                "interleaved duplicate warning notification should be suppressed"
            );
        }
    }
    assert_v2_suppressed_notification_metric(&metrics, "warning", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_interleaved_multi_worker_visible_notice_notifications() {
    let cases = vec![
        (
            "configWarning",
            serde_json::json!({
                "summary": "first shared config warning",
                "details": null,
            }),
            serde_json::json!({
                "summary": "second shared config warning",
                "details": "different config warning detail",
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "first shared deprecation notice",
                "details": null,
            }),
            serde_json::json!({
                "summary": "second shared deprecation notice",
                "details": "different deprecation detail",
            }),
        ),
    ];

    for (method, first_params, second_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, first_params.clone()),
            (method, second_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications_after_delay(
            vec![(method, first_params.clone())],
            Duration::from_millis(50),
        )
        .await;
        let metrics = in_memory_metrics();
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            first_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            second_params,
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Some(Ok(Message::Text(text)))) = timeout(remaining, websocket.next()).await
            else {
                break;
            };
            let message =
                serde_json::from_str::<JSONRPCMessage>(&text).expect("text frame should decode");
            if let JSONRPCMessage::Notification(notification) = message {
                assert!(
                    notification.method != method
                        || notification.params.as_ref() != Some(&first_params),
                    "interleaved duplicate {method} notification should be suppressed"
                );
            }
        }
        assert_v2_suppressed_notification_metric(&metrics, method, "duplicate");

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_preserves_same_worker_repeated_connection_notifications() {
    let cases = vec![
        (
            "account/updated",
            serde_json::json!({
                "authMode": null,
                "planType": null,
            }),
            serde_json::json!({
                "authMode": "apikey",
                "planType": null,
            }),
        ),
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "shared",
                    "limitName": "Shared",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "other",
                    "limitName": "Other",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "account/login/completed",
            serde_json::json!({
                "loginId": null,
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "loginId": "other-login",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [],
            }),
            serde_json::json!({
                "data": [{
                    "id": "other-app",
                    "name": "Other App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
        ),
        (
            "configWarning",
            serde_json::json!({
                "summary": "repeated worker config warning",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker config warning",
                "details": "different detail",
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "repeated worker deprecation notice",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker deprecation notice",
                "details": "different detail",
            }),
        ),
        (
            "externalAgentConfig/import/completed",
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
        ),
        (
            "mcpServer/oauthLogin/completed",
            serde_json::json!({
                "name": "shared-mcp",
                "success": true,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "mcpServer/startupStatus/updated",
            serde_json::json!({
                "name": "shared-mcp",
                "status": "ready",
                "error": null,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "status": "failed",
                "error": "startup failed",
            }),
        ),
        (
            "warning",
            serde_json::json!({
                "message": "repeated worker warning",
                "threadId": null,
            }),
            serde_json::json!({
                "message": "other worker warning",
                "threadId": null,
            }),
        ),
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/repeated-world-writable"],
                "extraCount": 1,
                "failedScan": false,
            }),
            serde_json::json!({
                "samplePaths": ["/tmp/other-world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "mode": "elevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, repeated_params, other_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, repeated_params.clone()),
            (method, other_params.clone()),
            (method, repeated_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications(Vec::new()).await;
        let metrics = in_memory_metrics();
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            repeated_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            other_params,
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            repeated_params,
        );
        assert_no_v2_suppressed_notification_metric(&metrics);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_config_warning_notifications()
{
    let notification_params = serde_json::json!({
        "summary": "shared config warning",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "configWarning",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "configWarning",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "configWarning",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "configWarning", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_deprecation_notice_notifications()
 {
    let notification_params = serde_json::json!({
        "summary": "shared deprecation notice",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "deprecationNotice",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "deprecationNotice", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_external_agent_import_completed_notifications()
 {
    let notification_params = serde_json::json!({
        "importId": "import-1",
        "itemTypeResults": [],
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
        notification_params.clone(),
    )
    .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "externalAgentConfig/import/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(
        &metrics,
        "externalAgentConfig/import/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_multi_worker_skills_changed_until_refresh() {
    let worker_a =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-a", vec!["skill-a"])
            .await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
            .await;
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
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(Duration::from_secs(1), websocket.next())
            .await
            .expect("expected skills/list response or refreshed notification")
            .expect("websocket message should exist")
            .expect("websocket message should decode");
        let Message::Text(text) = message else {
            continue;
        };
        let parsed =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("json-rpc message should decode");
        match parsed {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after skills/list refresh: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(post_refresh_duplicate.is_err(), true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_keeps_multi_worker_skills_changed_pending_after_failed_refresh() {
    let worker_a = start_mock_remote_server_for_skills_changed_and_failing_list().await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("failed-skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reopens_multi_worker_skills_changed_after_failed_then_successful_refresh()
 {
    let worker_a = start_mock_remote_server_for_skills_changed_failing_then_successful_list().await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
            .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("failed-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;
    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(duplicate.is_err());

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("successful-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(
            Duration::from_secs(1),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("skills/list response or refreshed notification should arrive");
        match message {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("successful-skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after successful skills/list: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(post_refresh_duplicate.is_err());

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_routes_multi_worker_plugin_management_to_first_successful_worker() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
        (
            "gitDiffToRemote",
            serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            }),
            serde_json::json!({
                "sha": "0123456789abcdef0123456789abcdef01234567",
                "diff": "diff --git a/README.md b/README.md\n",
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            expected_result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("plugin management request should serialize")
                .into(),
            ))
            .await
            .expect("plugin management request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected plugin management response for {method}");
        };
        assert_eq!(response.id, RequestId::String(format!("{method}-request")));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_forwards_chatgpt_auth_tokens_refresh_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("refresh-request-1".to_string()),
                    method: "account/chatgptAuthTokens/refresh".to_string(),
                    params: Some(serde_json::json!({
                        "reason": "unauthorized",
                        "previousAccountId": "acct-123",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("refresh response should exist")
            .expect("refresh response should decode")
        else {
            panic!("expected refresh response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("refresh response should decode")
        else {
            panic!("expected refresh response");
        };
        assert_eq!(
            response.id,
            RequestId::String("refresh-request-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::json!({
                "accessToken": "access-token-1",
                "chatgptAccountId": "acct-123",
                "chatgptPlanType": "pro",
            })
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 8,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    let JSONRPCMessage::Request(request) = read_websocket_message(&mut websocket).await else {
        panic!("expected forwarded refresh request");
    };
    assert_eq!(
        request.id,
        RequestId::String("refresh-request-1".to_string())
    );
    assert_eq!(request.method, "account/chatgptAuthTokens/refresh");
    assert_json_params_eq(
        request.params,
        Some(serde_json::json!({
            "reason": "unauthorized",
            "previousAccountId": "acct-123",
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::json!({
                    "accessToken": "access-token-1",
                    "chatgptAccountId": "acct-123",
                    "chatgptPlanType": "pro",
                }),
            }))
            .expect("refresh response should serialize")
            .into(),
        ))
        .await
        .expect("refresh response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_exec_command_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-exec-approval-1".to_string()),
                    method: "execCommandApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "approvalId": "approval-visible",
                        "command": ["echo", "hello"],
                        "cwd": "/tmp/workspace",
                        "reason": "Need to run a visible command",
                        "parsedCmd": [{
                            "type": "unknown",
                            "cmd": "echo hello",
                        }],
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-exec-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ExecCommandApprovalResponse {
                decision: ReviewDecision::Approved,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-exec-approval-1".to_string()),
                result: serde_json::to_value(ExecCommandApprovalResponse {
                    decision: ReviewDecision::Approved,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_apply_patch_approval_server_request_roundtrip() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("legacy-patch-approval-1".to_string()),
                    method: "applyPatchApproval".to_string(),
                    params: Some(serde_json::json!({
                        "conversationId": "thread-visible",
                        "callId": "call-visible",
                        "fileChanges": {
                            "README.md": {
                                "changeType": "added",
                                "oldContent": null,
                                "newContent": "hello\\n",
                            },
                        },
                        "reason": "Need to write visible changes",
                        "grantRoot": "/tmp/workspace",
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let Message::Text(text) = websocket
            .next()
            .await
            .expect("approval response should exist")
            .expect("approval response should decode")
        else {
            panic!("expected approval response text frame");
        };
        let JSONRPCMessage::Response(response) =
            serde_json::from_str(&text).expect("approval response should decode")
        else {
            panic!("expected approval response");
        };
        assert_eq!(
            response.id,
            RequestId::String("legacy-patch-approval-1".to_string())
        );
        assert_eq!(
            response.result,
            serde_json::to_value(ApplyPatchApprovalResponse {
                decision: ReviewDecision::ApprovedForSession,
            })
            .expect("approval response should serialize")
        );
    });

    let initialize_response = test_initialize_response().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: format!("ws://{downstream_addr}"),
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: RequestId::String("legacy-patch-approval-1".to_string()),
                result: serde_json::to_value(ApplyPatchApprovalResponse {
                    decision: ReviewDecision::ApprovedForSession,
                })
                .expect("approval response should serialize"),
            }))
            .expect("approval response should serialize")
            .into(),
        ))
        .await
        .expect("approval response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_fails_closed_when_server_request_answer_targets_exhausted_account() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let downstream_addr = listener.local_addr().expect("listener address");
    let (downstream_answer_tx, downstream_answer_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;

        let thread_start_request = read_websocket_request(&mut websocket).await;
        assert_eq!(thread_start_request.method, "thread/start");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: thread_start_request.id,
                    result: serde_json::json!({
                        "thread": {
                            "id": "thread-worker-a",
                            "forkedFromId": null,
                            "preview": "/tmp/worker-a",
                            "ephemeral": true,
                            "modelProvider": "openai",
                            "createdAt": 1,
                            "updatedAt": 1,
                            "status": {
                                "type": "idle",
                            },
                            "path": null,
                            "cwd": "/tmp/worker-a",
                            "cliVersion": "0.0.0-test",
                            "source": "cli",
                            "agentNickname": null,
                            "agentRole": null,
                            "gitInfo": null,
                            "name": null,
                            "turns": [],
                        },
                        "model": "gpt-5",
                        "modelProvider": "openai",
                        "serviceTier": null,
                        "cwd": "/tmp/worker-a",
                        "instructionSources": [],
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {
                            "type": "dangerFullAccess",
                        },
                        "reasoningEffort": null,
                    }),
                }))
                .expect("thread/start response should serialize")
                .into(),
            ))
            .await
            .expect("thread/start response should send");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String("srv-user-input".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "thread-worker-a",
                        "turnId": "turn-worker-a",
                        "itemId": "tool-call-worker-a",
                        "questions": [{
                            "id": "mode",
                            "header": "Mode",
                            "question": "Pick execution mode",
                            "isOther": false,
                            "isSecret": false,
                            "options": [],
                        }],
                    })),
                    trace: None,
                }))
                .expect("server request should serialize")
                .into(),
            ))
            .await
            .expect("server request should send");

        let maybe_response = timeout(Duration::from_millis(300), websocket.next())
            .await
            .ok()
            .and_then(|frame| frame)
            .and_then(Result::ok)
            .and_then(|message| match message {
                Message::Text(text) => Some(text.to_string()),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok(),
                Message::Close(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => None,
            });
        downstream_answer_tx
            .send(maybe_response)
            .expect("downstream answer observation should send");
    });

    let worker_b = start_mock_remote_server_for_initialize().await;
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            format!("ws://{downstream_addr}"),
            Some("acct-a".to_string()),
        ),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: observability.clone(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(
            GatewayV2SessionFactory::remote_multi_with_account_ids(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: format!("ws://{downstream_addr}"),
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
            .with_worker_health(worker_health.clone()),
        )),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    send_initialized(&mut websocket).await;
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-start".to_string()),
                method: "thread/start".to_string(),
                params: Some(serde_json::json!({
                    "model": null,
                    "modelProvider": null,
                    "serviceTier": null,
                    "cwd": "/tmp/worker-a",
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "sandbox": null,
                    "config": null,
                    "serviceName": null,
                    "baseInstructions": null,
                    "developerInstructions": null,
                    "personality": null,
                    "ephemeral": true,
                    "sessionStartSource": null,
                    "dynamicTools": null,
                    "experimentalRawEvents": false,
                    "persistExtendedHistory": false,
                })),
                trace: None,
            }))
            .expect("thread/start request should serialize")
            .into(),
        ))
        .await
        .expect("thread/start request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread/start response");
    };
    assert_eq!(response.id, RequestId::String("thread-start".to_string()));

    let JSONRPCMessage::Request(user_input_request) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected forwarded user-input request");
    };
    assert_eq!(user_input_request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        user_input_request.params,
        Some(serde_json::json!({
            "threadId": "thread-worker-a",
            "turnId": "turn-worker-a",
            "itemId": "tool-call-worker-a",
            "questions": [{
                "id": "mode",
                "header": "Mode",
                "question": "Pick execution mode",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    worker_health.mark_account_exhausted_for_worker(0, "quota reached".to_string());
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: user_input_request.id,
                result: serde_json::json!({
                    "answers": {
                        "mode": {
                            "answers": ["safe"],
                        },
                    },
                }),
            }))
            .expect("user-input response should serialize")
            .into(),
        ))
        .await
        .expect("user-input response should send");

    match timeout(Duration::from_secs(2), websocket.next())
        .await
        .expect("websocket should close or reset")
    {
        Some(Ok(Message::Close(Some(close_frame)))) => {
            assert_eq!(u16::from(close_frame.code), close_code::ERROR);
            assert!(
                    close_frame.reason.contains(
                        "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
                    ),
                    "{}",
                    close_frame.reason
                );
        }
        Some(Ok(Message::Close(None))) | None => {}
        Some(Err(err)) => {
            assert!(
                err.to_string().contains("reset without closing handshake"),
                "{err}"
            );
        }
        Some(Ok(message)) => panic!("expected websocket close or reset, got {message:?}"),
    }
    assert_eq!(
        downstream_answer_rx
            .await
            .expect("downstream answer observation should arrive"),
        None
    );
    assert_v2_server_request_answer_account_exhaustion_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "response", 1),
            (
                "downstream_server_request_forwarded",
                "item/tool/requestUserInput",
                1,
            ),
        ],
        &[("response", 1)],
        &[(0, "active_thread_handoff_failure", 1)],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-a"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "default",
            "projectId": null,
            "method": "serverRequest/respond",
            "threadId": "thread-worker-a",
            "exhaustedWorkerId": 0,
            "exhaustedAccountId": "acct-a",
            "reason": "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond",
        })
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("active_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("active_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("default")
    );
    assert_eq!(health.last_account_capacity_event_project_id, None);
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-a is pinned to worker 0 with exhausted account capacity for serverRequest/respond"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_forwarding_server_requests() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_thread_start_then_server_requests().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    send_initialized(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-start".to_string()),
                method: "thread/start".to_string(),
                params: Some(serde_json::json!({
                    "model": null,
                    "modelProvider": null,
                    "serviceTier": null,
                    "cwd": "/tmp/recovered-worker",
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "sandbox": null,
                    "config": null,
                    "serviceName": null,
                    "baseInstructions": null,
                    "developerInstructions": null,
                    "personality": null,
                    "ephemeral": true,
                    "sessionStartSource": null,
                    "dynamicTools": null,
                    "experimentalRawEvents": false,
                    "persistExtendedHistory": false,
                })),
                trace: None,
            }))
            .expect("thread/start request should serialize")
            .into(),
        ))
        .await
        .expect("thread/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("thread/start response should arrive") else {
        panic!("expected thread/start response");
    };
    assert_eq!(response.id, RequestId::String("thread-start".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-recovered",
                "forkedFromId": null,
                "preview": "",
                "ephemeral": true,
                "modelProvider": "openai",
                "createdAt": 1,
                "updatedAt": 1,
                "status": {
                    "type": "idle",
                },
                "path": null,
                "cwd": "/tmp/recovered-worker",
                "cliVersion": "0.0.0-test",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": null,
                "turns": [],
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/recovered-worker",
            "instructionSources": [],
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "sandbox": {
                "type": "dangerFullAccess",
            },
            "reasoningEffort": null,
        })
    );

    let JSONRPCMessage::Request(user_input_request) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("forwarded user-input request should arrive") else {
        panic!("expected forwarded user-input request");
    };
    assert_eq!(user_input_request.method, "item/tool/requestUserInput");
    assert_json_params_eq(
        user_input_request.params,
        Some(serde_json::json!({
            "threadId": "thread-recovered",
            "turnId": "turn-recovered",
            "itemId": "tool-call-recovered",
            "questions": [{
                "id": "mode",
                "header": "Mode",
                "question": "Pick execution mode",
                "isOther": false,
                "isSecret": false,
                "options": [],
            }],
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: user_input_request.id,
                result: serde_json::json!({
                    "answers": {
                        "mode": {
                            "answers": ["safe"],
                        },
                    },
                }),
            }))
            .expect("user-input response should serialize")
            .into(),
        ))
        .await
        .expect("user-input response should send");

    let JSONRPCMessage::Request(refresh_request) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("forwarded chatgpt refresh request should arrive") else {
        panic!("expected forwarded chatgpt refresh request");
    };
    assert_eq!(refresh_request.method, "account/chatgptAuthTokens/refresh");
    assert_json_params_eq(
        refresh_request.params,
        Some(serde_json::json!({
            "reason": "unauthorized",
            "previousAccountId": "acct-recovered",
        })),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                id: refresh_request.id,
                result: serde_json::json!({
                    "accessToken": "access-token-recovered",
                    "chatgptAccountId": "acct-recovered",
                    "chatgptPlanType": "pro",
                }),
            }))
            .expect("refresh response should serialize")
            .into(),
        ))
        .await
        .expect("refresh response should send");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_forwarding_login_completed() {
    let worker_a = start_mock_remote_server_for_reconnectable_primary_login_completed().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("account-login-start".to_string()),
                method: "account/login/start".to_string(),
                params: Some(serde_json::json!({
                    "type": "chatgpt",
                })),
                trace: None,
            }))
            .expect("account/login/start request should serialize")
            .into(),
        ))
        .await
        .expect("account/login/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("account/login/start response should arrive") else {
        panic!("expected account/login/start response");
    };
    assert_eq!(
        response.id,
        RequestId::String("account-login-start".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "type": "chatgpt",
            "loginId": "login-reconnected",
            "authUrl": "https://example.com/login",
        })
    );

    assert_jsonrpc_notification(
        timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("account/login/completed notification should arrive"),
        "account/login/completed",
        serde_json::json!({
            "loginId": "login-reconnected",
            "success": true,
            "error": null,
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_forwarding_mcp_oauth_completed() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "mcpServer/oauth/login",
        serde_json::json!({
            "name": "shared-mcp",
        }),
        JSONRPCErrorError {
            code: super::super::super::INVALID_PARAMS_CODE,
            message: "mcpServer/oauth/login missing on worker-a".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_mcp_oauth_login_completed().await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("mcp-oauth-login".to_string()),
                method: "mcpServer/oauth/login".to_string(),
                params: Some(serde_json::json!({
                    "name": "shared-mcp",
                })),
                trace: None,
            }))
            .expect("mcpServer/oauth/login request should serialize")
            .into(),
        ))
        .await
        .expect("mcpServer/oauth/login request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("mcpServer/oauth/login response should arrive") else {
        panic!("expected mcpServer/oauth/login response");
    };
    assert_eq!(
        response.id,
        RequestId::String("mcp-oauth-login".to_string())
    );
    assert_eq!(
        response.result,
        serde_json::json!({
            "authorizationUrl": "https://example.com/oauth/shared-mcp",
        })
    );

    assert_jsonrpc_notification(
        timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("mcpServer/oauthLogin/completed notification should arrive"),
        "mcpServer/oauthLogin/completed",
        serde_json::json!({
            "name": "shared-mcp",
            "success": true,
        }),
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_forwards_mcp_startup_status_from_reconnected_worker() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_mcp_server_status_list(vec!["worker-a-mcp"])
            .await;
    let worker_b =
            start_mock_remote_server_for_disconnect_then_reconnectable_mcp_status_with_startup_notification(
            )
            .await;
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("mcp-status-list".to_string()),
                method: "mcpServerStatus/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 25,
                    "detail": "toolsAndAuthOnly",
                })),
                trace: None,
            }))
            .expect("mcpServerStatus/list request should serialize")
            .into(),
        ))
        .await
        .expect("mcpServerStatus/list request should send");

    let mut saw_status_response = false;
    let mut saw_startup_notification = false;
    for _ in 0..2 {
        match timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("mcp status response or startup notification should arrive")
        {
            JSONRPCMessage::Response(response) => {
                assert_eq!(
                    response.id,
                    RequestId::String("mcp-status-list".to_string())
                );
                let mut status: ListMcpServerStatusResponse =
                    serde_json::from_value(response.result)
                        .expect("mcp status response should decode");
                status.data.sort_by(|a, b| a.name.cmp(&b.name));
                assert_eq!(status.next_cursor, None);
                assert_eq!(status.data.len(), 2);
                assert_eq!(status.data[0].name, "worker-a-mcp");
                assert_eq!(status.data[1].name, "worker-b-mcp");
                saw_status_response = true;
            }
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "mcpServer/startupStatus/updated");
                assert_eq!(
                    notification.params,
                    Some(serde_json::json!({
                        "threadId": null,
                        "name": "worker-b-mcp",
                        "status": "ready",
                        "error": null,
                    }))
                );
                saw_startup_notification = true;
            }
            message => panic!("unexpected gateway message: {message:?}"),
        }
    }

    assert_eq!(saw_status_response, true);
    assert_eq!(saw_startup_notification, true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_plugin_management_fallback_requests() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: super::super::super::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                params.clone(),
                expected_result.clone(),
            )
            .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("plugin management request should serialize")
                .into(),
            ))
            .await
            .expect("plugin management request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("plugin management response should arrive") else {
            panic!("expected plugin management response for {method}");
        };
        assert_eq!(response.id, RequestId::String(format!("{method}-request")));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_config_read_by_matching_cwd() {
    let params = serde_json::json!({
        "includeLayers": true,
        "cwd": "/tmp/worker-b/subdir",
    });
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
    let worker_b = start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
        "config/read",
        params.clone(),
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
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(params),
                trace: None,
            }))
            .expect("config/read request should serialize")
            .into(),
        ))
        .await
        .expect("config/read request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("config/read response should arrive") else {
        panic!("expected config/read response");
    };
    assert_eq!(response.id, RequestId::String("config-read".to_string()));
    assert_eq!(
        response.result,
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
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_threadless_config_read() {
    let params = serde_json::json!({
        "includeLayers": true,
        "cwd": null,
    });
    let worker_a = start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
        "config/read",
        params.clone(),
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
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("threadless-config-read".to_string()),
                method: "config/read".to_string(),
                params: Some(params),
                trace: None,
            }))
            .expect("config/read request should serialize")
            .into(),
        ))
        .await
        .expect("config/read request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("config/read response should arrive") else {
        panic!("expected config/read response");
    };
    assert_eq!(
        response.id,
        RequestId::String("threadless-config-read".to_string())
    );
    assert_eq!(
        response.result,
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
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_capability_requests() {
    let cases = vec![
        (
            "experimentalFeature/list",
            "experimental-feature-list",
            serde_json::json!({
                "cursor": null,
                "limit": 20,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-feature",
                    "stage": "beta",
                    "displayName": "Worker A Feature",
                    "description": "From worker A",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-feature",
                    "stage": "beta",
                    "displayName": "Worker B Feature",
                    "description": "From worker B",
                    "announcement": null,
                    "enabled": true,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-feature",
                        "stage": "beta",
                        "displayName": "Worker A Feature",
                        "description": "From worker A",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": false,
                    },
                    {
                        "name": "worker-b-feature",
                        "stage": "beta",
                        "displayName": "Worker B Feature",
                        "description": "From worker B",
                        "announcement": null,
                        "enabled": true,
                        "defaultEnabled": false,
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "collaborationMode/list",
            "collaboration-mode-list",
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-default",
                    "mode": "default",
                    "model": "gpt-5-worker-a",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-default",
                    "mode": "plan",
                    "model": "gpt-5-worker-b",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-default",
                        "mode": "default",
                        "model": "gpt-5-worker-a",
                        "reasoning_effort": null,
                    },
                    {
                        "name": "worker-b-default",
                        "mode": "plan",
                        "model": "gpt-5-worker-b",
                        "reasoning_effort": null,
                    }
                ],
            }),
        ),
    ];

    for (
        method,
        request_id,
        northbound_params,
        downstream_params,
        worker_a_result,
        worker_b_result,
        expected_result,
    ) in cases
    {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                downstream_params,
                worker_b_result,
            )
            .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(northbound_params),
                    trace: None,
                }))
                .expect("capability request should serialize")
                .into(),
            ))
            .await
            .expect("capability request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("capability response should arrive") else {
            panic!("expected capability response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_bootstrap_requests() {
    let cases = vec![
        (
            "account/read",
            "account-read",
            serde_json::json!({
                "refreshToken": false,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-a@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": false,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-b@example.com",
                    "planType": "enterprise",
                },
                "requiresOpenaiAuth": true,
            }),
            serde_json::json!({
                "account": {
                    "type": "chatgpt",
                    "email": "worker-a@example.com",
                    "planType": "plus",
                },
                "requiresOpenaiAuth": true,
            }),
        ),
        (
            "getAuthStatus",
            "get-auth-status",
            serde_json::json!({
                "includeToken": true,
                "refreshToken": false,
            }),
            serde_json::json!({
                "authMethod": "chatgpt",
                "authToken": "primary-token",
                "requiresOpenaiAuth": false,
            }),
            serde_json::json!({
                "authMethod": null,
                "authToken": null,
                "requiresOpenaiAuth": true,
            }),
            serde_json::json!({
                "authMethod": "chatgpt",
                "authToken": "primary-token",
                "requiresOpenaiAuth": true,
            }),
        ),
        (
            "account/rateLimits/read",
            "account-rate-limits-read",
            serde_json::json!({}),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 20,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": 20,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_000,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "worker-b",
                    "limitName": "Worker B",
                    "primary": {
                        "usedPercent": 35,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_500,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "worker-b": {
                        "limitId": "worker-b",
                        "limitName": "Worker B",
                        "primary": {
                            "usedPercent": 35,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_500,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": "Codex",
                    "primary": {
                        "usedPercent": 20,
                        "windowMinutes": 300,
                        "resetsAt": 1_700_000_000,
                    },
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": {
                    "codex": {
                        "limitId": "codex",
                        "limitName": "Codex",
                        "primary": {
                            "usedPercent": 20,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_000,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    },
                    "worker-b": {
                        "limitId": "worker-b",
                        "limitName": "Worker B",
                        "primary": {
                            "usedPercent": 35,
                            "windowMinutes": 300,
                            "resetsAt": 1_700_000_500,
                        },
                        "secondary": null,
                        "credits": null,
                        "planType": null,
                        "rateLimitReachedType": null,
                    }
                },
            }),
        ),
        (
            "model/list",
            "model-list",
            serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": true,
            }),
            serde_json::json!({
                "data": [reconnectable_model_json("worker-a-model", "Worker A Model", true)],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [reconnectable_model_json("worker-b-model", "Worker B Model", false)],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-a-model", "Worker A Model", true),
                    reconnectable_model_json("worker-b-model", "Worker B Model", false),
                ],
                "nextCursor": null,
            }),
        ),
        (
            "externalAgentConfig/detect",
            "external-agent-config-detect",
            serde_json::json!({
                "includeHome": true,
                "cwds": ["/tmp/project"],
            }),
            serde_json::json!({
                "items": [{
                    "itemType": "AGENTS_MD",
                    "description": "Import AGENTS.md from /tmp/worker-a",
                    "cwd": "/tmp/worker-a",
                    "details": null,
                }],
            }),
            serde_json::json!({
                "items": [{
                    "itemType": "CONFIG",
                    "description": "Import config from /tmp/worker-b",
                    "cwd": "/tmp/worker-b",
                    "details": null,
                }],
            }),
            serde_json::json!({
                "items": [
                    {
                        "itemType": "AGENTS_MD",
                        "description": "Import AGENTS.md from /tmp/worker-a",
                        "cwd": "/tmp/worker-a",
                        "details": null,
                    },
                    {
                        "itemType": "CONFIG",
                        "description": "Import config from /tmp/worker-b",
                        "cwd": "/tmp/worker-b",
                        "details": null,
                    }
                ],
            }),
        ),
        (
            "skills/list",
            "skills-list",
            serde_json::json!({
                "cwds": ["/tmp/project"],
                "perCwdExtraUserRoots": null,
            }),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
            serde_json::json!({
                "data": [{
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }],
            }),
            serde_json::json!({
                "data": [
                    {
                        "cwd": "/tmp/worker-a",
                        "skills": [{
                            "name": "skill-a",
                            "description": "skill-a description",
                            "path": "/tmp/worker-a/skill-a",
                            "scope": "repo",
                            "enabled": true,
                        }],
                        "errors": [],
                    },
                    {
                        "cwd": "/tmp/worker-b",
                        "skills": [{
                            "name": "skill-b",
                            "description": "skill-b description",
                            "path": "/tmp/worker-b/skill-b",
                            "scope": "repo",
                            "enabled": true,
                        }],
                        "errors": [],
                    }
                ],
            }),
        ),
        (
            "app/list",
            "app-list",
            serde_json::json!({
                "cursor": null,
                "limit": 25,
                "threadId": null,
            }),
            serde_json::json!({
                "data": [{
                    "id": "worker-a-app",
                    "name": "Worker A App",
                    "description": "Worker A App description",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "id": "worker-b-app",
                    "name": "Worker B App",
                    "description": "Worker B App description",
                    "installUrl": null,
                    "needsAuth": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "id": "worker-a-app",
                        "name": "Worker A App",
                        "description": "Worker A App description",
                        "installUrl": null,
                        "needsAuth": false,
                    },
                    {
                        "id": "worker-b-app",
                        "name": "Worker B App",
                        "description": "Worker B App description",
                        "installUrl": null,
                        "needsAuth": false,
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "plugin/list",
            "plugin-list",
            serde_json::json!({
                "cwds": ["/tmp/project"],
            }),
            serde_json::json!({
                "marketplaces": [{
                    "name": "worker-a-marketplace",
                    "path": "/tmp/project/worker-a-marketplace.json",
                    "interface": {
                        "displayName": "Worker A Marketplace",
                    },
                    "plugins": [{
                        "id": "worker-a-plugin@local",
                        "name": "worker-a-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/worker-a-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Worker A Plugin",
                            "shortDescription": "Worker A plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": ["worker-a-plugin@local"],
            }),
            serde_json::json!({
                "marketplaces": [{
                    "name": "worker-b-marketplace",
                    "path": "/tmp/project/worker-b-marketplace.json",
                    "interface": {
                        "displayName": "Worker B Marketplace",
                    },
                    "plugins": [{
                        "id": "worker-b-plugin@local",
                        "name": "worker-b-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/worker-b-plugin",
                        },
                        "installed": true,
                        "enabled": true,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Worker B Plugin",
                            "shortDescription": "Worker B plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    }],
                }],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": ["worker-b-plugin@local"],
            }),
            serde_json::json!({
                "marketplaces": [
                    {
                        "name": "worker-a-marketplace",
                        "path": "/tmp/project/worker-a-marketplace.json",
                        "interface": {
                            "displayName": "Worker A Marketplace",
                        },
                        "plugins": [{
                            "id": "worker-a-plugin@local",
                            "name": "worker-a-plugin",
                            "source": {
                                "type": "local",
                                "path": "/tmp/project/worker-a-plugin",
                            },
                            "installed": false,
                            "enabled": false,
                            "installPolicy": "AVAILABLE",
                            "authPolicy": "ON_USE",
                            "interface": {
                                "displayName": "Worker A Plugin",
                                "shortDescription": "Worker A plugin",
                                "longDescription": null,
                                "developerName": null,
                                "category": null,
                                "capabilities": [],
                                "websiteUrl": null,
                                "privacyPolicyUrl": null,
                                "termsOfServiceUrl": null,
                                "defaultPrompt": null,
                                "brandColor": null,
                                "composerIcon": null,
                                "composerIconUrl": null,
                                "logo": null,
                                "logoUrl": null,
                                "screenshots": [],
                                "screenshotUrls": [],
                            },
                        }],
                    },
                    {
                        "name": "worker-b-marketplace",
                        "path": "/tmp/project/worker-b-marketplace.json",
                        "interface": {
                            "displayName": "Worker B Marketplace",
                        },
                        "plugins": [{
                            "id": "worker-b-plugin@local",
                            "name": "worker-b-plugin",
                            "source": {
                                "type": "local",
                                "path": "/tmp/project/worker-b-plugin",
                            },
                            "installed": true,
                            "enabled": true,
                            "installPolicy": "AVAILABLE",
                            "authPolicy": "ON_USE",
                            "interface": {
                                "displayName": "Worker B Plugin",
                                "shortDescription": "Worker B plugin",
                                "longDescription": null,
                                "developerName": null,
                                "category": null,
                                "capabilities": [],
                                "websiteUrl": null,
                                "privacyPolicyUrl": null,
                                "termsOfServiceUrl": null,
                                "defaultPrompt": null,
                                "brandColor": null,
                                "composerIcon": null,
                                "composerIconUrl": null,
                                "logo": null,
                                "logoUrl": null,
                                "screenshots": [],
                                "screenshotUrls": [],
                            },
                        }],
                    }
                ],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [
                    "worker-a-plugin@local",
                    "worker-b-plugin@local",
                ],
            }),
        ),
        (
            "mcpServerStatus/list",
            "mcp-server-status-list",
            serde_json::json!({
                "cursor": null,
                "limit": 25,
                "detail": "toolsAndAuthOnly",
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-mcp",
                    "tools": {},
                    "resources": [],
                    "resourceTemplates": [],
                    "authStatus": "bearerToken",
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-mcp",
                    "tools": {},
                    "resources": [],
                    "resourceTemplates": [],
                    "authStatus": "bearerToken",
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-mcp",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "bearerToken",
                    },
                    {
                        "name": "worker-b-mcp",
                        "tools": {},
                        "resources": [],
                        "resourceTemplates": [],
                        "authStatus": "bearerToken",
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "thread/realtime/listVoices",
            "realtime-list-voices",
            serde_json::json!({}),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper", "maple"],
                    "v2": ["alloy"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
            serde_json::json!({
                "voices": {
                    "v1": ["maple", "cove"],
                    "v2": ["alloy", "marin"],
                    "defaultV1": "cove",
                    "defaultV2": "marin",
                },
            }),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper", "maple", "cove"],
                    "v2": ["alloy", "marin"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
        ),
        (
            "fuzzyFileSearch",
            "fuzzy-file-search",
            serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-reconnected",
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/worker-a",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 40,
                    "indices": [5, 6, 7, 8],
                }],
            }),
            serde_json::json!({
                "files": [{
                    "root": "/tmp/worker-b",
                    "path": "src/gateway.rs",
                    "match_type": "file",
                    "file_name": "gateway.rs",
                    "score": 60,
                    "indices": [4, 5, 6, 7],
                }],
            }),
            serde_json::json!({
                "files": [
                    {
                        "root": "/tmp/worker-b",
                        "path": "src/gateway.rs",
                        "match_type": "file",
                        "file_name": "gateway.rs",
                        "score": 60,
                        "indices": [4, 5, 6, 7],
                    },
                    {
                        "root": "/tmp/worker-a",
                        "path": "docs/gateway.md",
                        "match_type": "file",
                        "file_name": "gateway.md",
                        "score": 40,
                        "indices": [5, 6, 7, 8],
                    }
                ],
            }),
        ),
    ];

    for (method, request_id, params, worker_a_result, worker_b_result, expected_result) in cases {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
                start_mock_remote_server_for_disconnect_then_passthrough_request_with_optional_params_and_result(
                    method,
                    match method {
                        "account/rateLimits/read" => None,
                        "app/list" => Some(serde_json::json!({
                            "cursor": null,
                            "limit": null,
                            "threadId": null,
                        })),
                        "mcpServerStatus/list" => Some(serde_json::json!({
                            "cursor": null,
                            "limit": null,
                            "detail": "toolsAndAuthOnly",
                        })),
                        _ => Some(params.clone()),
                    },
                    worker_b_result,
                )
                .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("bootstrap request should serialize")
                .into(),
            ))
            .await
            .expect("bootstrap request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("bootstrap response should arrive") else {
            panic!("expected bootstrap response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        match method {
            "account/rateLimits/read" => {
                let response: GetAccountRateLimitsResponse =
                    serde_json::from_value(response.result)
                        .expect("rate limits response should decode");
                assert_eq!(response.rate_limits.limit_id.as_deref(), Some("codex"));
                assert_eq!(response.rate_limits.limit_name.as_deref(), Some("Codex"));
                assert_eq!(
                    response.rate_limits_by_limit_id.as_ref().map(HashMap::len),
                    Some(2)
                );
                assert_eq!(
                    response
                        .rate_limits_by_limit_id
                        .as_ref()
                        .and_then(|limits| limits.get("worker-b"))
                        .and_then(|limits| limits.limit_name.as_deref()),
                    Some("Worker B")
                );
            }
            "app/list" => {
                let mut response: AppsListResponse = serde_json::from_value(response.result)
                    .expect("app/list response should decode");
                response.data.sort_by(|a, b| a.id.cmp(&b.id));
                assert_eq!(response.next_cursor, None);
                assert_eq!(response.data.len(), 2);
                assert_eq!(response.data[0].id, "worker-a-app");
                assert_eq!(response.data[0].name, "Worker A App");
                assert_eq!(response.data[1].id, "worker-b-app");
                assert_eq!(response.data[1].name, "Worker B App");
            }
            _ => {
                let mut actual = response.result;
                let mut expected_result = expected_result;
                canonicalize_bootstrap_response_json(&mut actual);
                canonicalize_bootstrap_response_json(&mut expected_result);
                assert_eq!(actual, expected_result);
            }
        }

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_thread_requests() {
    let cases = vec![
        (
            "thread/list",
            "thread-list",
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": 10,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": null,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-a",
                    "Worker A thread",
                    "/tmp/worker-a",
                    1,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-b",
                    "Worker B thread",
                    "/tmp/worker-b",
                    2,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            "thread-loaded-list",
            serde_json::json!({
                "cursor": null,
                "limit": 10,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-a"],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-b"],
                "nextCursor": null,
            }),
        ),
    ];

    for (
        method,
        request_id,
        northbound_params,
        downstream_params,
        worker_a_result,
        worker_b_result,
    ) in cases
    {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                downstream_params,
                worker_b_result,
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
        scope_registry.register_thread("thread-worker-b".to_string(), context.clone());
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(northbound_params.clone()),
                    trace: None,
                }))
                .expect("thread aggregation request should serialize")
                .into(),
            ))
            .await
            .expect("thread aggregation request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("thread aggregation response should arrive") else {
            panic!("expected thread aggregation response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));

        match method {
            "thread/list" => {
                let listed: ThreadListResponse = serde_json::from_value(response.result)
                    .expect("thread/list response should decode");
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
            }
            "thread/loaded/list" => {
                let loaded: ThreadLoadedListResponse = serde_json::from_value(response.result)
                    .expect("thread/loaded/list response should decode");
                assert_eq!(loaded.next_cursor, None);
                assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);
            }
            other => panic!("unexpected thread aggregation method: {other}"),
        }

        assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_sticky_thread_requests() {
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            serde_json::json!({}),
        ),
        (
            "turn/steer",
            "turn-steer",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "expectedTurnId": "turn-reconnected",
                "input": [{
                    "type": "text",
                    "text": "continue",
                }],
            }),
            serde_json::json!({}),
        ),
        (
            "turn/interrupt",
            "turn-interrupt",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-reconnected",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky thread request should serialize")
                .into(),
            ))
            .await
            .expect("sticky thread request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky thread response should arrive") else {
            panic!("expected sticky thread response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_additional_sticky_thread_controls() {
    let cases = vec![
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
        ),
        (
            "thread/archive",
            "thread-archive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/unarchive",
            "thread-unarchive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
            }),
        ),
        (
            "thread/metadata/update",
            "thread-metadata-update",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "gitInfo": {
                    "sha": "abc123",
                    "branch": "main",
                    "originUrl": null,
                },
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "gitInfo": {
                        "commitHash": "abc123",
                        "branchName": "main",
                        "remoteUrl": null,
                    },
                },
            }),
        ),
        (
            "thread/turns/list",
            "thread-turns-list",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "cursor": null,
                "limit": 20,
                "sortDirection": "desc",
            }),
            serde_json::json!({
                "data": [{
                    "id": "turn-1",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": 1,
                    "completedAt": 2,
                    "durationMs": 1,
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/increment_elicitation",
            "thread-increment-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 2,
                "paused": true,
            }),
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
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/shellCommand",
            "thread-shell-command",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "command": "git status --short",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/backgroundTerminals/clean",
            "thread-background-terminals-clean",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/rollback",
            "thread-rollback",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "numTurns": 2,
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
            }),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky thread control request should serialize")
                .into(),
            ))
            .await
            .expect("sticky thread control request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky thread control response should arrive") else {
            panic!("expected sticky thread control response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_review_start_and_backfills_review_thread_route()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b =
        start_mock_remote_server_for_disconnect_then_reconnectable_review_start_then_thread_read()
            .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::clone(&scope_registry),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("review-start".to_string()),
                method: "review/start".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-worker-b",
                    "target": {
                        "type": "custom",
                        "instructions": "Review the current change",
                    },
                    "delivery": "detached",
                })),
                trace: None,
            }))
            .expect("review/start request should serialize")
            .into(),
        ))
        .await
        .expect("review/start request should send");

    let JSONRPCMessage::Response(review_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("review/start response should arrive") else {
        panic!("expected review/start response");
    };
    assert_eq!(
        review_response.id,
        RequestId::String("review-start".to_string())
    );
    assert_eq!(
        review_response.result,
        serde_json::json!({
            "turn": {
                "id": "turn-review",
                "items": [],
                "status": "pending",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null,
            },
            "reviewThreadId": "thread-review",
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-read-review".to_string()),
                method: "thread/read".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-review",
                    "includeTurns": false,
                })),
                trace: None,
            }))
            .expect("thread/read request should serialize")
            .into(),
        ))
        .await
        .expect("thread/read request should send");

    let JSONRPCMessage::Response(thread_read_response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("thread/read response should arrive") else {
        panic!("expected thread/read response");
    };
    assert_eq!(
        thread_read_response.id,
        RequestId::String("thread-read-review".to_string())
    );
    assert_eq!(
        thread_read_response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-review",
                "name": "Detached review thread",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_sticky_realtime_requests() {
    let cases = vec![
        (
            "thread/realtime/start",
            "realtime-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "outputModality": "text",
                "transport": {
                    "type": "websocket"
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendText",
            "realtime-append-text",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "text": "hello realtime",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/appendAudio",
            "realtime-append-audio",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 3,
                    "itemId": "item-visible",
                }
            }),
            serde_json::json!({}),
        ),
        (
            "thread/realtime/stop",
            "realtime-stop",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
        );
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("sticky realtime request should serialize")
                .into(),
            ))
            .await
            .expect("sticky realtime request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("sticky realtime response should arrive") else {
            panic!("expected sticky realtime response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_turn_start() {
    let expected_result = serde_json::json!({
        "turn": {
            "id": "turn-reconnected",
        },
    });
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
            "turn/start",
            expected_result.clone(),
        )
        .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry: Arc::clone(&scope_registry),
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("turn-start".to_string()),
                method: "turn/start".to_string(),
                params: Some(serde_json::json!({
                    "approvalPolicy": null,
                    "approvalsReviewer": null,
                    "collaborationMode": null,
                    "input": [],
                    "cwd": "/tmp/worker-b",
                    "effort": null,
                    "model": "gpt-5",
                    "outputSchema": null,
                    "personality": null,
                    "responsesapiClientMetadata": null,
                    "sandboxPolicy": null,
                    "summary": null,
                    "threadId": "thread-worker-b",
                })),
                trace: None,
            }))
            .expect("turn/start request should serialize")
            .into(),
        ))
        .await
        .expect("turn/start request should send");

    let JSONRPCMessage::Response(response) = timeout(
        Duration::from_secs(2),
        read_websocket_message(&mut websocket),
    )
    .await
    .expect("turn/start response should arrive") else {
        panic!("expected turn/start response");
    };
    assert_eq!(response.id, RequestId::String("turn-start".to_string()));
    assert_eq!(response.result, expected_result);

    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
    assert_eq!(
        *worker_b_requests.lock().await,
        vec!["turn/start".to_string()]
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_primary_worker_requests() {
    let cases = vec![
        (
            "configRequirements/read",
            "config-requirements-read",
            None,
            serde_json::json!({
                "requirements": [],
                "validationErrors": [],
            }),
        ),
        (
            "account/login/start",
            "account-login-start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-reconnected",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            "account-login-cancel",
            Some(serde_json::json!({
                "loginId": "login-reconnected",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "command/exec",
            "command-exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                "processId": "proc-reconnected",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            "command-exec-write",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            "command-exec-resize",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            "command-exec-terminate",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "feedback/upload",
            "feedback-upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway reconnect regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-reconnected",
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            "fuzzy-file-search-session-start",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            "fuzzy-file-search-session-update",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            "fuzzy-file-search-session-stop",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            "windows-sandbox-setup-start",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
            }),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                serde_json::json!({
                    "worker": "unexpected-secondary",
                }),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = params
            .as_ref()
            .and_then(|value| value.get("threadId"))
            .and_then(Value::as_str)
        {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: params.clone(),
                    trace: None,
                }))
                .expect("primary-worker request should serialize")
                .into(),
            ))
            .await
            .expect("primary-worker request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("primary-worker response should arrive") else {
            panic!("expected primary-worker response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);
        assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());

        server_task.abort();
        let _ = server_task.await;
    }
}
