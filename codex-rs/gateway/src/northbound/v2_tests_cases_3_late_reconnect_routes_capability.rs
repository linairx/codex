use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_capability_requests() {
    let cases = vec![
        (
            "experimentalFeature/list",
            "experimental-feature-list",
            serde_json::json!({
                "cursor": null,
                "limit": 20,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-feature",
                    "stage": "beta",
                    "displayName": "Worker A Feature",
                    "description": "From worker A",
                    "announcement": null,
                    "enabled": false,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-feature",
                    "stage": "beta",
                    "displayName": "Worker B Feature",
                    "description": "From worker B",
                    "announcement": null,
                    "enabled": true,
                    "defaultEnabled": false,
                }],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-feature",
                        "stage": "beta",
                        "displayName": "Worker A Feature",
                        "description": "From worker A",
                        "announcement": null,
                        "enabled": false,
                        "defaultEnabled": false,
                    },
                    {
                        "name": "worker-b-feature",
                        "stage": "beta",
                        "displayName": "Worker B Feature",
                        "description": "From worker B",
                        "announcement": null,
                        "enabled": true,
                        "defaultEnabled": false,
                    }
                ],
                "nextCursor": null,
            }),
        ),
        (
            "collaborationMode/list",
            "collaboration-mode-list",
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({
                "data": [{
                    "name": "worker-a-default",
                    "mode": "default",
                    "model": "gpt-5-worker-a",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [{
                    "name": "worker-b-default",
                    "mode": "plan",
                    "model": "gpt-5-worker-b",
                    "reasoningEffort": null,
                }],
            }),
            serde_json::json!({
                "data": [
                    {
                        "name": "worker-a-default",
                        "mode": "default",
                        "model": "gpt-5-worker-a",
                        "reasoning_effort": null,
                    },
                    {
                        "name": "worker-b-default",
                        "mode": "plan",
                        "model": "gpt-5-worker-b",
                        "reasoning_effort": null,
                    }
                ],
            }),
        ),
    ];

    for (
        method,
        request_id,
        northbound_params,
        downstream_params,
        worker_a_result,
        worker_b_result,
        expected_result,
    ) in cases
    {
        let worker_a =
            start_mock_remote_server_for_reconnectable_request(method, worker_a_result).await;
        let worker_b =
            start_mock_remote_server_for_disconnect_then_passthrough_request_with_result(
                method,
                downstream_params,
                worker_b_result,
            )
            .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
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
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(northbound_params),
                    trace: None,
                }))
                .expect("capability request should serialize")
                .into(),
            ))
            .await
            .expect("capability request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("capability response should arrive") else {
            panic!("expected capability response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}
