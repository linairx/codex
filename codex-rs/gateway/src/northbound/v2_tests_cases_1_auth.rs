use super::*;
use crate::northbound::v2_aggregation::aggregate_get_auth_status_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_get_auth_status_response_merges_multi_worker_auth_requirement() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getAuthStatus",
        serde_json::json!({
            "includeToken": true,
            "refreshToken": false,
        }),
        serde_json::json!({
            "authMethod": "chatgpt",
            "authToken": "primary-token",
            "requiresOpenaiAuth": false,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "getAuthStatus",
        serde_json::json!({
            "includeToken": true,
            "refreshToken": false,
        }),
        serde_json::json!({
            "authMethod": null,
            "authToken": null,
            "requiresOpenaiAuth": true,
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

    let response = aggregate_get_auth_status_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("get-auth-status".to_string()),
            method: "getAuthStatus".to_string(),
            params: Some(serde_json::json!({
                "includeToken": true,
                "refreshToken": false,
            })),
            trace: None,
        },
    )
    .await
    .expect("getAuthStatus aggregation should succeed");

    assert_eq!(
        response,
        serde_json::json!({
            "authMethod": "chatgpt",
            "authToken": "primary-token",
            "requiresOpenaiAuth": true,
        })
    );
}
