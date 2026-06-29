use super::support::*;
use pretty_assertions::assert_eq;

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
