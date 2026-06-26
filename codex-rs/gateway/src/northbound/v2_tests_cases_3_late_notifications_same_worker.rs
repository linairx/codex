use super::*;

#[tokio::test]
async fn websocket_upgrade_preserves_same_worker_repeated_connection_notifications() {
    let cases = vec![
        (
            "account/updated",
            serde_json::json!({
                "authMode": null,
                "planType": null,
            }),
            serde_json::json!({
                "authMode": "apikey",
                "planType": null,
            }),
        ),
        (
            "account/rateLimits/updated",
            serde_json::json!({
                "rateLimits": {
                    "limitId": "shared",
                    "limitName": "Shared",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
            serde_json::json!({
                "rateLimits": {
                    "limitId": "other",
                    "limitName": "Other",
                    "primary": null,
                    "secondary": null,
                    "credits": null,
                    "planType": null,
                    "rateLimitReachedType": null,
                },
            }),
        ),
        (
            "account/login/completed",
            serde_json::json!({
                "loginId": null,
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "loginId": "other-login",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "app/list/updated",
            serde_json::json!({
                "data": [],
            }),
            serde_json::json!({
                "data": [{
                    "id": "other-app",
                    "name": "Other App",
                    "description": null,
                    "logoUrl": null,
                    "logoUrlDark": null,
                    "distributionChannel": null,
                    "branding": null,
                    "appMetadata": null,
                    "labels": null,
                    "installUrl": null,
                    "isAccessible": false,
                    "isEnabled": true,
                    "pluginDisplayNames": [],
                }],
            }),
        ),
        (
            "configWarning",
            serde_json::json!({
                "summary": "repeated worker config warning",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker config warning",
                "details": "different detail",
            }),
        ),
        (
            "deprecationNotice",
            serde_json::json!({
                "summary": "repeated worker deprecation notice",
                "details": null,
            }),
            serde_json::json!({
                "summary": "other worker deprecation notice",
                "details": "different detail",
            }),
        ),
        (
            "externalAgentConfig/import/completed",
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
            serde_json::json!({
                "importId": "import-1",
                "itemTypeResults": [],
            }),
        ),
        (
            "mcpServer/oauthLogin/completed",
            serde_json::json!({
                "name": "shared-mcp",
                "success": true,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "success": false,
                "error": "cancelled",
            }),
        ),
        (
            "mcpServer/startupStatus/updated",
            serde_json::json!({
                "name": "shared-mcp",
                "status": "ready",
                "error": null,
            }),
            serde_json::json!({
                "name": "other-mcp",
                "status": "failed",
                "error": "startup failed",
            }),
        ),
        (
            "warning",
            serde_json::json!({
                "message": "repeated worker warning",
                "threadId": null,
            }),
            serde_json::json!({
                "message": "other worker warning",
                "threadId": null,
            }),
        ),
        (
            "windows/worldWritableWarning",
            serde_json::json!({
                "samplePaths": ["/tmp/repeated-world-writable"],
                "extraCount": 1,
                "failedScan": false,
            }),
            serde_json::json!({
                "samplePaths": ["/tmp/other-world-writable"],
                "extraCount": 2,
                "failedScan": false,
            }),
        ),
        (
            "windowsSandbox/setupCompleted",
            serde_json::json!({
                "mode": "unelevated",
                "success": true,
                "error": null,
            }),
            serde_json::json!({
                "mode": "elevated",
                "success": false,
                "error": "setup failed",
            }),
        ),
    ];

    for (method, repeated_params, other_params) in cases {
        let worker_a = start_mock_remote_server_for_realtime_notifications(vec![
            (method, repeated_params.clone()),
            (method, other_params.clone()),
            (method, repeated_params.clone()),
        ])
        .await;
        let worker_b = start_mock_remote_server_for_realtime_notifications(Vec::new()).await;
        let metrics = in_memory_metrics();
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::new(Some(metrics.clone()), false),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;

        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            repeated_params.clone(),
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            other_params,
        );
        assert_jsonrpc_notification(
            read_websocket_message(&mut websocket).await,
            method,
            repeated_params,
        );
        assert_no_v2_suppressed_notification_metric(&metrics);

        server_task.abort();
        let _ = server_task.await;
    }
}
