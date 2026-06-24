use super::*;
use crate::northbound::v2_request_dispatch::handle_client_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn aggregated_discovery_requests_fail_closed_during_reconnect_backoff() {
    let cases = vec![
        (
            "account/read",
            Some(serde_json::json!({
                "refreshToken": false,
            })),
            serde_json::json!({
                "account": null,
                "requiresOpenaiAuth": false,
            }),
        ),
        (
            "getAuthStatus",
            Some(serde_json::json!({
                "includeToken": false,
                "refreshToken": false,
            })),
            serde_json::json!({
                "authMethod": null,
                "authToken": null,
                "requiresOpenaiAuth": false,
            }),
        ),
        (
            "account/rateLimits/read",
            None,
            serde_json::json!({
                "rateLimits": {
                    "limitId": null,
                    "limitName": null,
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
                "rateLimitsByLimitId": null,
            }),
        ),
        (
            "app/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "threadId": null,
                "forceRefetch": false,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "mcpServerStatus/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "detail": "toolsAndAuthOnly",
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "externalAgentConfig/detect",
            Some(serde_json::json!({})),
            serde_json::json!({
                "items": [],
            }),
        ),
        (
            "model/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
                "includeHidden": null,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "skills/list",
            Some(serde_json::json!({})),
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "experimentalFeature/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": null,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
        (
            "collaborationMode/list",
            Some(serde_json::json!({})),
            serde_json::json!({
                "data": [],
            }),
        ),
        (
            "plugin/list",
            Some(serde_json::json!({
                "cwds": null,
            })),
            serde_json::json!({
                "marketplaces": [],
                "marketplaceLoadErrors": [],
                "featuredPluginIds": [],
            }),
        ),
        (
            "thread/realtime/listVoices",
            Some(serde_json::json!({})),
            serde_json::json!({
                "voices": {
                    "v1": ["juniper"],
                    "v2": ["alloy"],
                    "defaultV1": "juniper",
                    "defaultV2": "alloy",
                },
            }),
        ),
    ];

    for (method, params, response) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                response.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                response.clone(),
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
        assert!(
            router.remove_worker(Some(1)),
            "test should drop a worker before applying reconnect backoff"
        );
        router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

        let metrics = in_memory_metrics();
        let admission = GatewayAdmissionController::default();
        let observability = GatewayObservability::new(Some(metrics.clone()), false);
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

        let err = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params,
                trace: None,
            },
        )
        .await
        .expect_err("aggregated discovery should fail closed during reconnect backoff");

        assert_eq!(
            err.to_string(),
            format!("required worker routes are unavailable for {method}: [1]")
        );
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
        assert_v2_fail_closed_request_metric(&metrics, method, true);
    }
}

#[tokio::test]
async fn degraded_thread_discovery_logs_unavailable_worker_routes() {
    let cases = vec![
        (
            "thread/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            serde_json::json!({
                "data": [],
                "nextCursor": null,
            }),
        ),
    ];

    for (method, params, response) in cases {
        let logs = capture_logs_async(async {
            let (worker_a, worker_a_requests) =
                start_mock_remote_server_for_reconnectable_request_with_recording(
                    method,
                    response.clone(),
                )
                .await;
            let (worker_b, worker_b_requests) =
                start_mock_remote_server_for_reconnectable_request_with_recording(
                    method,
                    response.clone(),
                )
                .await;
            let scope_registry = Arc::new(GatewayScopeRegistry::default());
            let context = GatewayRequestContext {
                tenant_id: "tenant-visible".to_string(),
                project_id: Some("project-visible".to_string()),
            };
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
            assert!(
                router.remove_worker(Some(1)),
                "test should drop a worker before applying reconnect backoff"
            );
            router.record_worker_reconnect_failure(1, Instant::now(), Duration::from_secs(60));

            let admission = GatewayAdmissionController::default();
            let metrics = in_memory_metrics();
            let observability = GatewayObservability::new(Some(metrics.clone()), false);
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

            let result = handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params,
                    trace: None,
                },
            )
            .await
            .expect("degraded thread discovery should reach available workers")
            .expect("degraded thread discovery should succeed");

            assert!(result.is_object());
            assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
            assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
            assert_v2_degraded_thread_discovery_metric(&metrics, method, true);
        })
        .await;

        assert!(
            logs.contains("serving degraded multi-worker thread discovery from available workers")
        );
        assert_eq!(
            logs.matches("serving degraded multi-worker thread discovery from available workers")
                .count(),
            1
        );
        assert!(logs.contains("tenant-visible"));
        assert!(logs.contains("project-visible"));
        assert!(logs.contains(&format!("method=\"{method}\"")));
        assert!(logs.contains("available_worker_ids=[0]"));
        assert!(logs.contains("available_worker_websocket_urls=["));
        assert!(logs.contains("unavailable_worker_ids=[1]"));
        assert!(logs.contains("unavailable_worker_websocket_urls=["));
        assert!(logs.contains("reconnect_backoff_worker_ids=[1]"));
        assert!(logs.contains("reconnect_backoff_worker_websocket_urls=["));
        assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
        assert!(logs.contains("reconnect_backoff_worker_routes=["));
    }
}
