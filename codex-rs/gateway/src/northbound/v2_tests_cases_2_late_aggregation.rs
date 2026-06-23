use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_skills_list() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_skills_list("/tmp/worker-a", vec!["skill-a"])
            .await;
    let worker_b =
        start_mock_remote_server_for_reconnectable_skills_list("/tmp/worker-b", vec!["skill-b"])
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
    let mut router = timeout(
        Duration::from_secs(2),
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context),
    )
    .await
    .expect("downstream router connect should finish in time")
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("skills-list".to_string()),
            method: "skills/list".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("skills/list should reach downstream workers")
    .expect("skills/list should succeed after reconnecting the missing worker");

    let mut response: SkillsListResponse =
        serde_json::from_value(result).expect("skills/list response should decode");
    response.data.sort_by(|a, b| a.cwd.cmp(&b.cwd));

    assert_eq!(router.worker_count(), 2);
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].cwd, PathBuf::from("/tmp/worker-a"));
    assert_eq!(response.data[0].skills.len(), 1);
    assert_eq!(response.data[0].skills[0].name, "skill-a");
    assert_eq!(response.data[1].cwd, PathBuf::from("/tmp/worker-b"));
    assert_eq!(response.data[1].skills.len(), 1);
    assert_eq!(response.data[1].skills[0].name, "skill-b");
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_fuzzy_file_search() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "fuzzyFileSearch",
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
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "fuzzyFileSearch",
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("fuzzy-file-search".to_string()),
            method: "fuzzyFileSearch".to_string(),
            params: Some(serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-reconnected",
            })),
            trace: None,
        },
    )
    .await
    .expect("fuzzyFileSearch should reach downstream workers")
    .expect("fuzzyFileSearch should succeed after reconnecting the missing worker");

    let response: FuzzyFileSearchResponse =
        serde_json::from_value(result).expect("fuzzyFileSearch response should decode");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        response.files,
        vec![
            FuzzyFileSearchResult {
                root: "/tmp/worker-b".to_string(),
                path: "src/gateway.rs".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.rs".to_string(),
                score: 60,
                indices: Some(vec![4, 5, 6, 7]),
            },
            FuzzyFileSearchResult {
                root: "/tmp/worker-a".to_string(),
                path: "docs/gateway.md".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.md".to_string(),
                score: 40,
                indices: Some(vec![5, 6, 7, 8]),
            },
        ]
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_app_list() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_app_list(vec![("worker-a-app", "Worker A App")])
            .await;
    let worker_b =
        start_mock_remote_server_for_reconnectable_app_list(vec![("worker-b-app", "Worker B App")])
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("app-list".to_string()),
            method: "app/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 25,
                "threadId": null,
            })),
            trace: None,
        },
    )
    .await
    .expect("app/list should reach downstream workers")
    .expect("app/list should succeed after reconnecting the missing worker");

    let mut response: AppsListResponse =
        serde_json::from_value(result).expect("app/list response should decode");
    response.data.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(router.worker_count(), 2);
    assert_eq!(response.next_cursor, None);
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].id, "worker-a-app");
    assert_eq!(response.data[0].name, "Worker A App");
    assert_eq!(response.data[1].id, "worker-b-app");
    assert_eq!(response.data[1].name, "Worker B App");
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_mcp_server_status_list()
{
    let worker_a =
        start_mock_remote_server_for_reconnectable_mcp_server_status_list(vec!["worker-a-mcp"])
            .await;
    let worker_b =
        start_mock_remote_server_for_reconnectable_mcp_server_status_list(vec!["worker-b-mcp"])
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("mcp-server-status-list".to_string()),
            method: "mcpServerStatus/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 25,
                "detail": "toolsAndAuthOnly",
            })),
            trace: None,
        },
    )
    .await
    .expect("mcpServerStatus/list should reach downstream workers")
    .expect("mcpServerStatus/list should succeed after reconnecting the missing worker");

    let mut response: ListMcpServerStatusResponse =
        serde_json::from_value(result).expect("mcpServerStatus/list response should decode");
    response.data.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(router.worker_count(), 2);
    assert_eq!(response.next_cursor, None);
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].name, "worker-a-mcp");
    assert_eq!(response.data[1].name, "worker-b-mcp");
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_plugin_list() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "plugin/list",
        serde_json::json!({
            "marketplaces": [{
                "name": "worker-a-marketplace",
                "path": "/tmp/worker-a/plugins/marketplace.json",
                "interface": {
                    "displayName": "Worker A Marketplace",
                },
                "plugins": [{
                    "id": "worker-a-plugin@local",
                    "name": "worker-a-plugin",
                    "source": {
                        "type": "local",
                        "path": "/tmp/worker-a/plugins/worker-a-plugin",
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
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "plugin/list",
        serde_json::json!({
            "marketplaces": [{
                "name": "worker-b-marketplace",
                "path": "/tmp/worker-b/plugins/marketplace.json",
                "interface": {
                    "displayName": "Worker B Marketplace",
                },
                "plugins": [{
                    "id": "worker-b-plugin@local",
                    "name": "worker-b-plugin",
                    "source": {
                        "type": "local",
                        "path": "/tmp/worker-b/plugins/worker-b-plugin",
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("plugin-list".to_string()),
            method: "plugin/list".to_string(),
            params: Some(serde_json::json!({
                "cwds": ["/tmp/project"],
            })),
            trace: None,
        },
    )
    .await
    .expect("plugin/list should reach downstream workers")
    .expect("plugin/list should succeed after reconnecting the missing worker");

    let response: PluginListResponse =
        serde_json::from_value(result).expect("plugin/list response should decode");
    let mut plugin_ids = response
        .marketplaces
        .iter()
        .flat_map(|marketplace| marketplace.plugins.iter().map(|plugin| plugin.id.clone()))
        .collect::<Vec<_>>();
    plugin_ids.sort();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(response.marketplaces.len(), 2);
    assert_eq!(
        plugin_ids,
        vec![
            "worker-a-plugin@local".to_string(),
            "worker-b-plugin@local".to_string(),
        ]
    );
    assert_eq!(
        response.featured_plugin_ids,
        vec![
            "worker-a-plugin@local".to_string(),
            "worker-b-plugin@local".to_string(),
        ]
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_account_read() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "account/read",
        serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "worker-a@example.com",
                "planType": "plus",
            },
            "requiresOpenaiAuth": false,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "account/read",
        serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "worker-b@example.com",
                "planType": "enterprise",
            },
            "requiresOpenaiAuth": true,
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("account-read".to_string()),
            method: "account/read".to_string(),
            params: Some(serde_json::json!({
                "refreshToken": false,
            })),
            trace: None,
        },
    )
    .await
    .expect("account/read should reach downstream workers")
    .expect("account/read should succeed after reconnecting the missing worker");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        result,
        serde_json::json!({
            "account": {
                "type": "chatgpt",
                "email": "worker-a@example.com",
                "planType": "plus",
            },
            "requiresOpenaiAuth": true,
        })
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_account_rate_limits_read()
 {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "account/rateLimits/read",
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
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "account/rateLimits/read",
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("account-rate-limits-read".to_string()),
            method: "account/rateLimits/read".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("account/rateLimits/read should reach downstream workers")
    .expect("account/rateLimits/read should succeed after reconnecting the missing worker");

    let response: GetAccountRateLimitsResponse =
        serde_json::from_value(result).expect("rate limits should decode");

    assert_eq!(router.worker_count(), 2);
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

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_model_list() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "model/list",
        serde_json::json!({
            "data": [reconnectable_model_json("worker-a-model", "Worker A Model", true)],
            "nextCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "model/list",
        serde_json::json!({
            "data": [reconnectable_model_json("worker-b-model", "Worker B Model", false)],
            "nextCursor": null,
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list should reach downstream workers")
    .expect("model/list should succeed after reconnecting the missing worker");

    let mut model_ids = result["data"]
        .as_array()
        .expect("model/list response should include data array")
        .iter()
        .map(|model| {
            model["model"]
                .as_str()
                .expect("model/list response should include model id")
                .to_string()
        })
        .collect::<Vec<_>>();
    model_ids.sort();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(result["nextCursor"], Value::Null);
    assert_eq!(
        model_ids,
        vec!["worker-a-model".to_string(), "worker-b-model".to_string()]
    );
}

#[tokio::test]
async fn handle_client_request_aggregates_paginated_multi_worker_model_list() {
    let worker_a = start_mock_remote_server_for_paginated_model_list(vec![
        (
            None,
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-a-model-1", "Worker A Model 1", true),
                ],
                "nextCursor": "worker-a-page-2",
            }),
        ),
        (
            Some("worker-a-page-2".to_string()),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("shared-model", "Shared Model From A", false),
                ],
                "nextCursor": null,
            }),
        ),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_paginated_model_list(vec![(
        None,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-b-model-1", "Worker B Model 1", false),
                reconnectable_model_json("shared-model", "Shared Model From B", false),
            ],
            "nextCursor": null,
        }),
    )])
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

    let first_page = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-page-1".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list first page should reach downstream workers")
    .expect("model/list first page should succeed");

    assert_eq!(
        first_page,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-a-model-1", "Worker A Model 1", true),
                reconnectable_model_json("shared-model", "Shared Model From A", false),
            ],
            "nextCursor": "model-offset:2",
        })
    );

    let second_page = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-page-2".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "model-offset:2",
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list second page should reach downstream workers")
    .expect("model/list second page should succeed");

    assert_eq!(
        second_page,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-b-model-1", "Worker B Model 1", false),
            ],
            "nextCursor": null,
        })
    );
}

#[tokio::test]
async fn handle_client_request_rejects_repeated_worker_pagination_cursor() {
    let worker = start_mock_remote_server_for_paginated_model_list(vec![
        (
            None,
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-model-1", "Worker Model 1", true),
                ],
                "nextCursor": "worker-loop",
            }),
        ),
        (
            Some("worker-loop".to_string()),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-model-2", "Worker Model 2", false),
                ],
                "nextCursor": "worker-loop",
            }),
        ),
    ])
    .await;
    let idle_worker = start_mock_remote_server_for_idle_session().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
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
            },
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: idle_worker,
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

    let error = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-loop".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect_err("repeated worker cursor should fail the aggregated request");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        error.to_string(),
        "downstream model/list returned repeated pagination cursor: worker-loop"
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_external_agent_config_detect()
 {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "externalAgentConfig/detect",
        serde_json::json!({
            "items": [{
                "itemType": "AGENTS_MD",
                "description": "Import AGENTS.md from /tmp/worker-a",
                "cwd": "/tmp/worker-a",
                "details": null,
            }],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "externalAgentConfig/detect",
        serde_json::json!({
            "items": [{
                "itemType": "CONFIG",
                "description": "Import config from /tmp/worker-b",
                "cwd": "/tmp/worker-b",
                "details": null,
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("external-agent-config-detect".to_string()),
            method: "externalAgentConfig/detect".to_string(),
            params: Some(serde_json::json!({
                "includeHome": true,
                "cwds": ["/tmp/project"],
            })),
            trace: None,
        },
    )
    .await
    .expect("externalAgentConfig/detect should reach downstream workers")
    .expect("externalAgentConfig/detect should succeed after reconnecting the missing worker");

    let mut descriptions = result["items"]
        .as_array()
        .expect("externalAgentConfig/detect response should include items")
        .iter()
        .map(|item| {
            item["description"]
                .as_str()
                .expect("externalAgentConfig/detect item should include description")
                .to_string()
        })
        .collect::<Vec<_>>();
    descriptions.sort();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        descriptions,
        vec![
            "Import AGENTS.md from /tmp/worker-a".to_string(),
            "Import config from /tmp/worker-b".to_string(),
        ]
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_experimental_feature_list()
 {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "experimentalFeature/list",
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
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "experimentalFeature/list",
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("experimental-feature-list".to_string()),
            method: "experimentalFeature/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 20,
            })),
            trace: None,
        },
    )
    .await
    .expect("experimentalFeature/list should reach downstream workers")
    .expect("experimentalFeature/list should succeed after reconnecting the missing worker");

    let response: ExperimentalFeatureListResponse =
        serde_json::from_value(result).expect("experimental features should decode");
    let feature_names = response
        .data
        .iter()
        .map(|feature| feature.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(feature_names, vec!["worker-a-feature", "worker-b-feature"]);
    assert_eq!(response.next_cursor, None);
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_collaboration_mode_list()
{
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "collaborationMode/list",
        serde_json::json!({
            "data": [{
                "name": "worker-a-default",
                "mode": "default",
                "model": "gpt-5-worker-a",
                "reasoningEffort": null,
            }],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "collaborationMode/list",
        serde_json::json!({
            "data": [{
                "name": "worker-b-default",
                "mode": "plan",
                "model": "gpt-5-worker-b",
                "reasoningEffort": null,
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("collaboration-mode-list".to_string()),
            method: "collaborationMode/list".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("collaborationMode/list should reach downstream workers")
    .expect("collaborationMode/list should succeed after reconnecting the missing worker");

    let response: CollaborationModeListResponse =
        serde_json::from_value(result).expect("collaboration modes should decode");
    let mode_names = response
        .data
        .iter()
        .map(|mode| mode.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(mode_names, vec!["worker-a-default", "worker-b-default"]);
}
