use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;

#[tokio::test]
async fn handle_client_request_routes_conversation_summary_by_visible_rollout_path() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/worker-b/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params.clone(),
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/worker-b/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params,
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
                "preview": "Worker B summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-b",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_path_with_worker(
        "/tmp/worker-b/rollout.jsonl",
        context.clone(),
        None,
    );

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

    let result = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-path".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(serde_json::json!({
                "rolloutPath": "/tmp/worker-b/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect("getConversationSummary should reach downstream workers")
    .expect("getConversationSummary should succeed through visible path discovery");

    assert_eq!(
        result["summary"]["conversationId"],
        serde_json::json!("thread-worker-b")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-b/rollout.jsonl"),
        Some(1)
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-a",
                "path": "/tmp/shared/rollout.jsonl",
                "preview": "Worker A summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

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
    .with_worker_health(worker_health);
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

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("conversation-summary-path".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(summary_params),
                    trace: None,
                },
            )
            .await
            .expect("getConversationSummary should try a replacement worker")
            .expect("getConversationSummary should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        response["summary"]["conversationId"],
        serde_json::json!("thread-worker-a")
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(0));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "path_thread_handoff_success")]);
    assert_v2_account_capacity_event_health(
        &observability,
        0,
        "path_thread_handoff_success",
        1,
        "tenant-a",
        Some("project-a"),
        "path-based thread request restored on a replacement account-backed worker",
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_rejects_replacement_worker_returning_wrong_path() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": "thread-worker-a",
                "path": "/tmp/other/rollout.jsonl",
                "preview": "Wrong worker summary",
                "timestamp": null,
                "updatedAt": null,
                "modelProvider": "openai",
                "cwd": "/tmp/worker-a",
                "cliVersion": "0.0.0-test",
                "source": "codex_cli",
                "gitInfo": null,
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

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
    .with_worker_health(worker_health);
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
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
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

    let error = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("conversation-summary-path".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary should fail closed on wrong restored path");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), None);
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(1)
    );
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/other/rollout.jsonl"),
        None
    );
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_records_handoff_failure_when_no_replacement_restores_context() {
    let summary_params = serde_json::json!({
        "rolloutPath": "/tmp/shared/rollout.jsonl",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params,
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: "thread not found: /tmp/shared/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());

    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_path_with_worker(
        "/tmp/shared/rollout.jsonl",
        context.clone(),
        Some(1),
    );

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
    .with_worker_health(worker_health);
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
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
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

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("conversation-summary-path".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(serde_json::json!({
                        "rolloutPath": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect_err("getConversationSummary should fail closed without a replacement worker"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffFailed");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}
