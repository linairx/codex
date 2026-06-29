use super::support::*;
use pretty_assertions::assert_eq;

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
