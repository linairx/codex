use super::*;
use crate::northbound::v2_aggregation_catalog::aggregate_fuzzy_file_search_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_fuzzy_file_search_response_merges_multi_worker_results() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "fuzzyFileSearch",
        serde_json::json!({
            "query": "gate",
            "roots": ["/tmp/project"],
            "cancellationToken": "search-1",
        }),
        serde_json::json!({
            "files": [
                {
                    "root": "/tmp/project-a",
                    "path": "docs/gateway.md",
                    "match_type": "file",
                    "file_name": "gateway.md",
                    "score": 40,
                    "indices": [5, 6, 7, 8],
                },
                {
                    "root": "/tmp/shared",
                    "path": "README.md",
                    "match_type": "file",
                    "file_name": "README.md",
                    "score": 10,
                    "indices": null,
                },
            ],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "fuzzyFileSearch",
        serde_json::json!({
            "query": "gate",
            "roots": ["/tmp/project"],
            "cancellationToken": "search-1",
        }),
        serde_json::json!({
            "files": [
                {
                    "root": "/tmp/project-b",
                    "path": "src/gateway.rs",
                    "match_type": "file",
                    "file_name": "gateway.rs",
                    "score": 60,
                    "indices": [4, 5, 6, 7],
                },
                {
                    "root": "/tmp/shared",
                    "path": "README.md",
                    "match_type": "file",
                    "file_name": "README.md",
                    "score": 25,
                    "indices": [0],
                },
            ],
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

    let response = aggregate_fuzzy_file_search_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("fuzzy-file-search".to_string()),
            method: "fuzzyFileSearch".to_string(),
            params: Some(serde_json::json!({
                "query": "gate",
                "roots": ["/tmp/project"],
                "cancellationToken": "search-1",
            })),
            trace: None,
        },
    )
    .await
    .expect("fuzzy file search aggregation should succeed");
    let response: FuzzyFileSearchResponse =
        serde_json::from_value(response).expect("fuzzy file search should decode");

    assert_eq!(
        response,
        FuzzyFileSearchResponse {
            files: vec![
                FuzzyFileSearchResult {
                    root: "/tmp/project-b".to_string(),
                    path: "src/gateway.rs".to_string(),
                    match_type: FuzzyFileSearchMatchType::File,
                    file_name: "gateway.rs".to_string(),
                    score: 60,
                    indices: Some(vec![4, 5, 6, 7]),
                },
                FuzzyFileSearchResult {
                    root: "/tmp/project-a".to_string(),
                    path: "docs/gateway.md".to_string(),
                    match_type: FuzzyFileSearchMatchType::File,
                    file_name: "gateway.md".to_string(),
                    score: 40,
                    indices: Some(vec![5, 6, 7, 8]),
                },
                FuzzyFileSearchResult {
                    root: "/tmp/shared".to_string(),
                    path: "README.md".to_string(),
                    match_type: FuzzyFileSearchMatchType::File,
                    file_name: "README.md".to_string(),
                    score: 25,
                    indices: Some(vec![0]),
                },
            ],
        }
    );
}
