use super::support::*;
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
