use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_worker_before_aggregated_thread_requests() {
    let cases = vec![
        (
            "thread/list",
            "thread-list",
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": 10,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "archived": null,
                "cursor": null,
                "cwd": null,
                "limit": null,
                "modelProviders": null,
                "searchTerm": null,
                "sortDirection": null,
                "sortKey": null,
                "sourceKinds": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-a",
                    "Worker A thread",
                    "/tmp/worker-a",
                    1,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
            serde_json::json!({
                "data": [reconnectable_thread_list_entry_json(
                    "thread-worker-b",
                    "Worker B thread",
                    "/tmp/worker-b",
                    2,
                )],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/loaded/list",
            "thread-loaded-list",
            serde_json::json!({
                "cursor": null,
                "limit": 10,
            }),
            serde_json::json!({
                "cursor": null,
                "limit": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-a"],
                "nextCursor": null,
            }),
            serde_json::json!({
                "data": ["thread-worker-b"],
                "nextCursor": null,
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
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
        scope_registry.register_thread("thread-worker-b".to_string(), context.clone());
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::clone(&scope_registry),
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
                    params: Some(northbound_params.clone()),
                    trace: None,
                }))
                .expect("thread aggregation request should serialize")
                .into(),
            ))
            .await
            .expect("thread aggregation request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("thread aggregation response should arrive") else {
            panic!("expected thread aggregation response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));

        match method {
            "thread/list" => {
                let listed: ThreadListResponse = serde_json::from_value(response.result)
                    .expect("thread/list response should decode");
                assert_eq!(listed.next_cursor, None);
                assert_eq!(listed.backwards_cursor, None);
                assert_eq!(
                    listed
                        .data
                        .iter()
                        .map(|thread| thread.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["thread-worker-b", "thread-worker-a"]
                );
            }
            "thread/loaded/list" => {
                let loaded: ThreadLoadedListResponse = serde_json::from_value(response.result)
                    .expect("thread/loaded/list response should decode");
                assert_eq!(loaded.next_cursor, None);
                assert_eq!(loaded.data, vec!["thread-worker-a", "thread-worker-b"]);
            }
            other => panic!("unexpected thread aggregation method: {other}"),
        }

        assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));

        server_task.abort();
        let _ = server_task.await;
    }
}
