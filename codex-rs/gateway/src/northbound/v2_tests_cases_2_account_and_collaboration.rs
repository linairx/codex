use super::*;
use crate::northbound::v2_aggregation::aggregate_account_rate_limits_response;
use crate::northbound::v2_aggregation_catalog::aggregate_collaboration_mode_list_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregate_account_rate_limits_response_merges_multi_worker_data() {
    let worker_a =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "account/rateLimits/read",
            None,
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
    let worker_b =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "account/rateLimits/read",
            None,
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
    let context = GatewayRequestContext::default();
    let observability = GatewayObservability::default();

    let response = aggregate_account_rate_limits_response(
        &router,
        &context,
        &observability,
        &JSONRPCRequest {
            id: RequestId::String("account-rate-limits-read".to_string()),
            method: "account/rateLimits/read".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("account rate limits aggregation should succeed");
    let response: GetAccountRateLimitsResponse =
        serde_json::from_value(response).expect("rate limits should decode");

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
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("codex"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Codex")
    );
    assert_eq!(
        response
            .rate_limits_by_limit_id
            .as_ref()
            .and_then(|rate_limits_by_limit_id| rate_limits_by_limit_id.get("worker-b"))
            .and_then(|snapshot| snapshot.limit_name.as_deref()),
        Some("Worker B")
    );
}

#[tokio::test]
async fn aggregate_account_rate_limits_response_updates_account_capacity() {
    let worker_a =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "account/rateLimits/read",
            None,
            serde_json::json!({
                "rateLimits": {
                    "limitId": "worker-a",
                    "limitName": "Worker A",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": null,
            }),
        )
        .await;
    let worker_b =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "account/rateLimits/read",
            None,
            serde_json::json!({
                "rateLimits": {
                    "limitId": "worker-b",
                    "limitName": "Worker B",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": "workspace_member_usage_limit_reached",
                },
                "rateLimitsByLimitId": null,
            }),
        )
        .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(0, "previous failure".to_string());
    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
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
        vec![Some("acct-a".to_string()), Some("acct-b".to_string())],
    )
    .with_worker_health(worker_health.clone());
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
    let context = GatewayRequestContext::default();
    let observability = GatewayObservability::default();

    aggregate_account_rate_limits_response(
        &router,
        &context,
        &observability,
        &JSONRPCRequest {
            id: RequestId::String("account-rate-limits-read".to_string()),
            method: "account/rateLimits/read".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("account rate limits aggregation should succeed");

    assert_eq!(worker_health.account_has_capacity(0), true);
    assert_eq!(worker_health.account_has_capacity(1), false);
    assert_eq!(
        worker_health.snapshot()[1]
            .account_capacity_reason
            .as_deref(),
        Some("account/rateLimits reported Worker B WorkspaceMemberUsageLimitReached")
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("available".to_string(), 1), ("exhausted".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, [("available".to_string(), 1)].into()),
            (1, [("exhausted".to_string(), 1)].into()),
        ]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("exhausted")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("default")
    );
    assert_eq!(health.last_account_capacity_event_project_id, None);
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("account/rateLimits reported Worker B WorkspaceMemberUsageLimitReached")
    );

    let available_response: GetAccountRateLimitsResponse =
        serde_json::from_value(serde_json::json!({
            "rateLimits": {
                "limitId": "worker-a",
                "limitName": "Worker A",
                "primary": null,
                "secondary": null,
                "credits": null,
                "planType": null,
                "rateLimitReachedType": null,
            },
            "rateLimitsByLimitId": null,
        }))
        .expect("available rate limits response should parse");
    let exhausted_response: GetAccountRateLimitsResponse =
        serde_json::from_value(serde_json::json!({
            "rateLimits": {
                "limitId": "worker-b",
                "limitName": "Worker B",
                "primary": null,
                "secondary": null,
                "credits": null,
                "planType": null,
                "rateLimitReachedType": "workspace_member_usage_limit_reached",
            },
            "rateLimitsByLimitId": null,
        }))
        .expect("exhausted rate limits response should parse");
    crate::northbound::v2_account_capacity::sync_worker_account_capacity_from_rate_limits_response(
        &router,
        &context,
        &observability,
        Some(0),
        &available_response,
    );
    crate::northbound::v2_account_capacity::sync_worker_account_capacity_from_rate_limits_response(
        &router,
        &context,
        &observability,
        Some(1),
        &exhausted_response,
    );

    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("available".to_string(), 1), ("exhausted".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, [("available".to_string(), 1)].into()),
            (1, [("exhausted".to_string(), 1)].into()),
        ]
    );
}

#[tokio::test]
async fn aggregate_collaboration_mode_list_response_deduplicates_and_sorts_multi_worker_data() {
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "collaborationMode/list",
        serde_json::json!({}),
        serde_json::json!({
            "data": [
                {
                    "name": "worker-a-default",
                    "mode": "default",
                    "model": "gpt-5-worker-a",
                    "reasoningEffort": null,
                },
                {
                    "name": "shared-mode",
                    "mode": "plan",
                    "model": "gpt-5-shared",
                    "reasoningEffort": null,
                }
            ],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "collaborationMode/list",
        serde_json::json!({}),
        serde_json::json!({
            "data": [
                {
                    "name": "worker-b-default",
                    "mode": "default",
                    "model": "gpt-5-worker-b",
                    "reasoningEffort": null,
                },
                {
                    "name": "shared-mode",
                    "mode": "plan",
                    "model": "gpt-5-shared",
                    "reasoningEffort": null,
                }
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

    let response = aggregate_collaboration_mode_list_response(
        &router,
        &JSONRPCRequest {
            id: RequestId::String("collaboration-mode-list".to_string()),
            method: "collaborationMode/list".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("collaboration mode aggregation should succeed");
    let response: CollaborationModeListResponse =
        serde_json::from_value(response).expect("collaboration modes should decode");
    assert_eq!(
        response
            .data
            .iter()
            .map(|mode| mode.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-mode", "worker-a-default", "worker-b-default"]
    );
}
