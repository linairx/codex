use super::*;
use crate::northbound::v2_aggregation_catalog::aggregate_realtime_list_voices_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_realtime_list_voices_response_merges_multi_worker_data() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/realtime/listVoices",
        serde_json::json!({}),
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
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/realtime/listVoices",
        serde_json::json!({}),
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

    let response = aggregate_realtime_list_voices_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("realtime-list-voices".to_string()),
            method: "thread/realtime/listVoices".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("realtime list voices aggregation should succeed");
    let response: ThreadRealtimeListVoicesResponse =
        serde_json::from_value(response).expect("realtime list voices should decode");
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
