use super::support::*;
use pretty_assertions::assert_eq;

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
