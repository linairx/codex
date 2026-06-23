use super::*;
use crate::northbound::v2_aggregation_catalog::aggregate_experimental_feature_list_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_experimental_feature_list_response_merges_and_sorts_multi_worker_data() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "experimentalFeature/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
        }),
        serde_json::json!({
            "data": [
                {
                    "name": "bravo-feature",
                    "stage": "beta",
                    "displayName": "Bravo Feature",
                    "description": "From worker A",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                },
                {
                    "name": "shared-feature",
                    "stage": "beta",
                    "displayName": "Shared Feature",
                    "description": "From worker A",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": true,
                }
            ],
            "nextCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "experimentalFeature/list",
        serde_json::json!({
            "cursor": null,
            "limit": null,
        }),
        serde_json::json!({
            "data": [
                {
                    "name": "alpha-feature",
                    "stage": "beta",
                    "displayName": "Alpha Feature",
                    "description": "From worker B",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                },
                {
                    "name": "shared-feature",
                    "stage": "beta",
                    "displayName": "Shared Feature",
                    "description": "From worker B",
                    "announcement": null,
                    "enabled": true,
                    "defaultEnabled": false,
                }
            ],
            "nextCursor": null,
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

    let first_page = aggregate_experimental_feature_list_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("experimental-features".to_string()),
            method: "experimentalFeature/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            trace: None,
        },
    )
    .await
    .expect("experimental feature aggregation should succeed");
    let first_page: ExperimentalFeatureListResponse =
        serde_json::from_value(first_page).expect("experimental features should decode");
    assert_eq!(
        first_page
            .data
            .iter()
            .map(|feature| (
                feature.name.as_str(),
                feature.enabled,
                feature.default_enabled
            ))
            .collect::<Vec<_>>(),
        vec![
            ("alpha-feature", false, false),
            ("bravo-feature", false, false),
            ("shared-feature", true, true),
        ]
    );
    assert_eq!(first_page.next_cursor, None);
}

#[tokio::test]
async fn aggregate_experimental_feature_list_response_drains_downstream_pages_before_gateway_pagination()
 {
    let worker_a = start_mock_remote_server_for_paginated_passthrough_requests(
        "experimentalFeature/list",
        vec![
            (
                serde_json::json!({
                    "cursor": null,
                    "limit": null,
                }),
                serde_json::json!({
                    "data": [{
                        "name": "zeta-feature",
                        "stage": "beta",
                        "displayName": "Zeta Feature",
                        "description": "From worker A page 1",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": false,
                    }],
                    "nextCursor": "worker-feature-page-2",
                }),
            ),
            (
                serde_json::json!({
                    "cursor": "worker-feature-page-2",
                    "limit": null,
                }),
                serde_json::json!({
                    "data": [{
                        "name": "shared-feature",
                        "stage": "beta",
                        "displayName": "Shared Feature",
                        "description": "From worker A page 2",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": true,
                    }],
                    "nextCursor": null,
                }),
            ),
        ],
    )
    .await;
    let worker_b = start_mock_remote_server_for_paginated_passthrough_requests(
        "experimentalFeature/list",
        vec![(
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "alpha-feature",
                        "stage": "beta",
                        "displayName": "Alpha Feature",
                        "description": "From worker B",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": false,
                    },
                    {
                        "name": "shared-feature",
                        "stage": "beta",
                        "displayName": "Shared Feature",
                        "description": "From worker B",
                        "announcement": null,
                        "enabled": true,
                        "defaultEnabled": false,
                    }
                ],
                "nextCursor": null,
            }),
        )],
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

    let page = aggregate_experimental_feature_list_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("experimental-features-page".to_string()),
            method: "experimentalFeature/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "experimental-feature-offset:1",
                "limit": 1,
            })),
            trace: None,
        },
    )
    .await
    .expect("experimental feature aggregation should succeed");
    let page: ExperimentalFeatureListResponse =
        serde_json::from_value(page).expect("experimental features should decode");
    assert_eq!(
        page.next_cursor.as_deref(),
        Some("experimental-feature-offset:2")
    );
    assert_eq!(
        page.data
            .iter()
            .map(|feature| (
                feature.name.as_str(),
                feature.enabled,
                feature.default_enabled
            ))
            .collect::<Vec<_>>(),
        vec![("shared-feature", true, true)]
    );
}
