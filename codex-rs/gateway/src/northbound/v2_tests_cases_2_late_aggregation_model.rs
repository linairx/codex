use super::support::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_model_list() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "model/list",
        serde_json::json!({
            "data": [reconnectable_model_json("worker-a-model", "Worker A Model", true)],
            "nextCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "model/list",
        serde_json::json!({
            "data": [reconnectable_model_json("worker-b-model", "Worker B Model", false)],
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
            id: RequestId::String("model-list".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list should reach downstream workers")
    .expect("model/list should succeed after reconnecting the missing worker");

    let mut model_ids = result["data"]
        .as_array()
        .expect("model/list response should include data array")
        .iter()
        .map(|model| {
            model["model"]
                .as_str()
                .expect("model/list response should include model id")
                .to_string()
        })
        .collect::<Vec<_>>();
    model_ids.sort();

    assert_eq!(router.worker_count(), 2);
    assert_eq!(result["nextCursor"], Value::Null);
    assert_eq!(
        model_ids,
        vec!["worker-a-model".to_string(), "worker-b-model".to_string()]
    );
}

#[tokio::test]
async fn handle_client_request_aggregates_paginated_multi_worker_model_list() {
    let worker_a = start_mock_remote_server_for_paginated_model_list(vec![
        (
            None,
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-a-model-1", "Worker A Model 1", true),
                ],
                "nextCursor": "worker-a-page-2",
            }),
        ),
        (
            Some("worker-a-page-2".to_string()),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("shared-model", "Shared Model From A", false),
                ],
                "nextCursor": null,
            }),
        ),
    ])
    .await;
    let worker_b = start_mock_remote_server_for_paginated_model_list(vec![(
        None,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-b-model-1", "Worker B Model 1", false),
                reconnectable_model_json("shared-model", "Shared Model From B", false),
            ],
            "nextCursor": null,
        }),
    )])
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

    let first_page = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-page-1".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list first page should reach downstream workers")
    .expect("model/list first page should succeed");

    assert_eq!(
        first_page,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-a-model-1", "Worker A Model 1", true),
                reconnectable_model_json("shared-model", "Shared Model From A", false),
            ],
            "nextCursor": "model-offset:2",
        })
    );

    let second_page = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-page-2".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": "model-offset:2",
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect("model/list second page should reach downstream workers")
    .expect("model/list second page should succeed");

    assert_eq!(
        second_page,
        serde_json::json!({
            "data": [
                reconnectable_model_json("worker-b-model-1", "Worker B Model 1", false),
            ],
            "nextCursor": null,
        })
    );
}

#[tokio::test]
async fn handle_client_request_rejects_repeated_worker_pagination_cursor() {
    let worker = start_mock_remote_server_for_paginated_model_list(vec![
        (
            None,
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-model-1", "Worker Model 1", true),
                ],
                "nextCursor": "worker-loop",
            }),
        ),
        (
            Some("worker-loop".to_string()),
            serde_json::json!({
                "data": [
                    reconnectable_model_json("worker-model-2", "Worker Model 2", false),
                ],
                "nextCursor": "worker-loop",
            }),
        ),
    ])
    .await;
    let idle_worker = start_mock_remote_server_for_idle_session().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    let session_factory = GatewayV2SessionFactory::remote_multi(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker,
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
                    websocket_url: idle_worker,
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

    let error = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("model-list-loop".to_string()),
            method: "model/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 2,
                "includeHidden": true,
            })),
            trace: None,
        },
    )
    .await
    .expect_err("repeated worker cursor should fail the aggregated request");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        error.to_string(),
        "downstream model/list returned repeated pagination cursor: worker-loop"
    );
}
