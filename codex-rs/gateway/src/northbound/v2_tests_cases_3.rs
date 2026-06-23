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
                            "logoDark": null,
                            "logoUrl": null,
                            "logoUrlDark": null,
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
                            "logoDark": null,
                            "logoUrl": null,
                            "logoUrlDark": null,
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

#[path = "v2_tests_cases_3_late.rs"]
mod v2_tests_cases_3_late;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_3_late::*;
