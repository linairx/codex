use super::*;
use crate::northbound::v2_aggregation::aggregate_thread_list_response;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_filters_thread_list_responses_by_scope() {
    let initialize_response = test_initialize_response().await;
    let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/list",
        serde_json::json!({
            "cursor": null,
            "limit": 10,
            "sortKey": null,
            "sortDirection": null,
            "modelProviders": null,
            "sourceKinds": null,
            "archived": null,
            "cwd": null,
            "searchTerm": null,
        }),
        serde_json::json!({
            "data": [
                {
                    "id": "thread-visible",
                    "name": "Visible thread",
                },
                {
                    "id": "thread-hidden",
                    "name": "Hidden thread",
                }
            ],
            "nextCursor": null,
            "backwardsCursor": null,
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::default(),
        scope_registry,
        session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_single(
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url,
                    auth_token: None,
                },
                client_name: "codex-gateway".to_string(),
                client_version: "0.0.0-test".to_string(),
                experimental_api: false,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: 4,
            },
            initialize_response,
        ))),
        timeouts: GatewayV2Timeouts::default(),
    })
    .await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("thread-list".to_string()),
                method: "thread/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 10,
                })),
                trace: None,
            }))
            .expect("thread list request should serialize")
            .into(),
        ))
        .await
        .expect("thread list request should send");

    let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await else {
        panic!("expected thread list response");
    };
    assert_eq!(response.id, RequestId::String("thread-list".to_string()));
    assert_eq!(
        response.result,
        serde_json::json!({
            "data": [
                {
                    "id": "thread-visible",
                    "name": "Visible thread",
                }
            ],
            "nextCursor": null,
            "backwardsCursor": null,
        })
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn aggregate_thread_list_response_backfills_multi_worker_routes_for_visible_threads() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "thread/list",
        serde_json::json!({
                "data": [{
                    "id": "thread-worker-a",
                    "sessionId": "thread-worker-a",
                    "forkedFromId": null,
                    "preview": "",
                    "ephemeral": true,
                "modelProvider": "openai",
                "createdAt": 1,
                "updatedAt": 1,
                "status": { "type": "idle" },
                "path": null,
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": "Worker A thread",
                "turns": [],
            }],
            "nextCursor": null,
            "backwardsCursor": null,
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "thread/list",
        serde_json::json!({
                "data": [{
                    "id": "thread-worker-b",
                    "sessionId": "thread-worker-b",
                    "forkedFromId": null,
                    "preview": "",
                    "ephemeral": true,
                "modelProvider": "openai",
                "createdAt": 2,
                "updatedAt": 2,
                "status": { "type": "idle" },
                "path": null,
                "cwd": "/tmp/worker-b",
                "cliVersion": "0.0.0-test",
                "source": "cli",
                "agentNickname": null,
                "agentRole": null,
                "gitInfo": null,
                "name": "Worker B thread",
                "turns": [],
            }],
            "nextCursor": null,
            "backwardsCursor": null,
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread("thread-worker-a".to_string(), context.clone());
    scope_registry.register_thread("thread-worker-b".to_string(), context);
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
    let list_response = aggregate_thread_list_response(
        &router,
        &scope_registry,
        &GatewayRequestContext::default(),
        &GatewayObservability::default(),
        &JSONRPCRequest {
            id: RequestId::String("thread-list".to_string()),
            method: "thread/list".to_string(),
            params: Some(serde_json::json!({
                "cursor": null,
                "limit": 10,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread list aggregation should succeed");
    let listed: codex_app_server_protocol::ThreadListResponse =
        serde_json::from_value(list_response).expect("thread list response should decode");
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
    assert_eq!(
        listed.data[0].cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b"
    );
    assert_eq!(
        listed.data[1].cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
}

#[tokio::test]
async fn aggregate_thread_list_response_deduplicates_same_thread_id_across_workers() {
    let logs = capture_logs_async(async {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
            "thread/list",
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
                "data": [{
                    "id": "thread-shared",
                    "sessionId": "thread-shared",
                    "forkedFromId": null,
                    "preview": "",
                    "ephemeral": true,
                    "modelProvider": "openai",
                    "createdAt": 1,
                    "updatedAt": 1,
                    "status": { "type": "idle" },
                    "path": null,
                    "cwd": "/tmp/worker-a",
                    "cliVersion": "0.0.0-test",
                    "source": "cli",
                    "agentNickname": null,
                    "agentRole": null,
                    "gitInfo": null,
                    "name": "Older copy",
                    "turns": [],
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
            "thread/list",
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
                "data": [{
                    "id": "thread-shared",
                    "sessionId": "thread-shared",
                    "forkedFromId": null,
                    "preview": "",
                    "ephemeral": true,
                    "modelProvider": "openai",
                    "createdAt": 1,
                    "updatedAt": 5,
                    "status": { "type": "idle" },
                    "path": null,
                    "cwd": "/tmp/worker-b",
                    "cliVersion": "0.0.0-test",
                    "source": "cli",
                    "agentNickname": null,
                    "agentRole": null,
                    "gitInfo": null,
                    "name": "Newer copy",
                    "turns": [],
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext {
            tenant_id: "tenant-visible".to_string(),
            project_id: Some("project-visible".to_string()),
        };
        scope_registry.register_thread("thread-shared".to_string(), context.clone());
        let metrics = in_memory_metrics();
        let observability = GatewayObservability::new(Some(metrics.clone()), false);
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
        let router =
            GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &context)
                .await
                .expect("downstream router should connect");
        let list_response = aggregate_thread_list_response(
            &router,
            &scope_registry,
            &context,
            &observability,
            &JSONRPCRequest {
                id: RequestId::String("thread-list".to_string()),
                method: "thread/list".to_string(),
                params: Some(serde_json::json!({
                    "cursor": null,
                    "limit": 10,
                })),
                trace: None,
            },
        )
        .await
        .expect("thread list aggregation should succeed");
        let listed: codex_app_server_protocol::ThreadListResponse =
            serde_json::from_value(list_response).expect("thread list response should decode");
        assert_eq!(listed.data.len(), 1);
        assert_eq!(listed.data[0].id, "thread-shared");
        assert_eq!(listed.data[0].name, Some("Newer copy".to_string()));
        assert_eq!(
            listed.data[0].cwd.as_ref().to_string_lossy(),
            "/tmp/worker-b"
        );
        assert_eq!(scope_registry.thread_worker_id("thread-shared"), Some(1));
        assert_v2_thread_list_deduplication_metric(&metrics, Some(1));
    })
    .await;

    assert!(logs.contains("deduplicating repeated thread/list entry across downstream workers"));
    assert!(logs.contains("tenant-visible"));
    assert!(logs.contains("project-visible"));
    assert!(logs.contains("thread-shared"));
    assert!(logs.contains("selected_worker_id=Some(1)"));
    assert!(logs.contains("discarded_worker_id=Some(0)"));
    assert!(logs.contains("selected_worker_websocket_url="));
    assert!(logs.contains("discarded_worker_websocket_url="));
    assert!(logs.contains("selected_updated_at=5"));
    assert!(logs.contains("discarded_updated_at=1"));
}
