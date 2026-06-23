use super::*;
use crate::northbound::v2_aggregation_catalog::aggregate_plugin_list_response;
use crate::northbound::v2_aggregation_catalog::merge_plugin_summary;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_plugin_list_response_merges_multi_worker_marketplaces() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "plugin/list",
        serde_json::json!({
            "cwds": ["/tmp/project"],
        }),
        serde_json::json!({
            "marketplaces": [{
                "name": "demo-marketplace",
                "path": "/tmp/project/plugins/demo-marketplace.json",
                "interface": {
                    "displayName": "Demo Marketplace",
                },
                "plugins": [{
                    "id": "shared-plugin@local",
                    "name": "shared-plugin",
                    "source": {
                        "type": "local",
                        "path": "/tmp/project/plugins/shared-plugin",
                    },
                    "installed": false,
                    "enabled": false,
                    "installPolicy": "AVAILABLE",
                    "authPolicy": "ON_USE",
                    "interface": {
                        "displayName": "Shared Plugin",
                        "shortDescription": "Shared plugin from worker A",
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
            "marketplaceLoadErrors": [{
                "marketplacePath": "/tmp/project/plugins/broken.json",
                "message": "failed to load worker-a marketplace",
            }],
            "featuredPluginIds": ["shared-plugin@local"],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "plugin/list",
        serde_json::json!({
            "cwds": ["/tmp/project"],
        }),
        serde_json::json!({
            "marketplaces": [{
                "name": "demo-marketplace",
                "path": "/tmp/project/plugins/demo-marketplace.json",
                "interface": {
                    "displayName": "Demo Marketplace",
                },
                "plugins": [
                    {
                        "id": "shared-plugin@local",
                        "name": "shared-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/shared-plugin",
                        },
                        "installed": true,
                        "enabled": true,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Shared Plugin",
                            "shortDescription": "Shared plugin from worker B",
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
                    {
                        "id": "worker-b-plugin@local",
                        "name": "worker-b-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/worker-b-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Worker B Plugin",
                            "shortDescription": "Worker B only plugin",
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
                    }
                ],
            }],
            "marketplaceLoadErrors": [{
                "marketplacePath": "/tmp/project/plugins/broken.json",
                "message": "failed to load worker-a marketplace",
            }],
            "featuredPluginIds": ["shared-plugin@local", "worker-b-plugin@local"],
        }),
    )
    .await;
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

    let response = aggregate_plugin_list_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("plugin-list".to_string()),
            method: "plugin/list".to_string(),
            params: Some(serde_json::json!({
                "cwds": ["/tmp/project"],
            })),
            trace: None,
        },
    )
    .await
    .expect("plugin list aggregation should succeed");
    let response: PluginListResponse =
        serde_json::from_value(response).expect("plugin list should decode");

    assert_eq!(response.marketplaces.len(), 1);
    assert_eq!(response.marketplaces[0].plugins.len(), 2);
    assert_eq!(
        response.marketplaces[0].plugins[0].id,
        "shared-plugin@local"
    );
    assert_eq!(response.marketplaces[0].plugins[0].installed, true);
    assert_eq!(response.marketplaces[0].plugins[0].enabled, true);
    assert_eq!(
        response.marketplaces[0].plugins[1].id,
        "worker-b-plugin@local"
    );
    assert_eq!(
        response.featured_plugin_ids,
        vec![
            "shared-plugin@local".to_string(),
            "worker-b-plugin@local".to_string(),
        ]
    );
    assert_eq!(response.marketplace_load_errors.len(), 1);
}

#[test]
fn merge_plugin_summary_preserves_installed_copy_across_worker_repeats() {
    let available =
        plugin_summary_json("shared-plugin@local", false, false, "Available worker copy");
    let installed = plugin_summary_json("shared-plugin@local", true, true, "Installed worker copy");
    let later_available = plugin_summary_json(
        "shared-plugin@local",
        false,
        false,
        "Later available worker copy",
    );
    let mut plugins = vec![available];

    merge_plugin_summary(&mut plugins, installed.clone());
    merge_plugin_summary(&mut plugins, later_available);

    assert_eq!(plugins, vec![installed]);
}
