use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn thread_turns_list_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_turns_list_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "cursor": null,
        "limit": 20,
        "sortDirection": "desc",
    });
    let turns_response = serde_json::json!({
        "data": [{
            "id": "turn-worker-b",
            "items": [],
            "status": "completed",
            "error": null,
            "startedAt": 1,
            "completedAt": 2,
            "durationMs": 1,
        }],
        "nextCursor": null,
        "backwardsCursor": null,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/turns/list",
        thread_turns_list_params.clone(),
        turns_response.clone(),
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
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
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

    let response = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-turns-list".to_string()),
            method: "thread/turns/list".to_string(),
            params: Some(thread_turns_list_params),
            trace: None,
        },
    )
    .await
    .expect("thread/turns/list should try a replacement worker")
    .expect("thread/turns/list should restore from replacement worker");

    assert_eq!(response, turns_response);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_turns_list_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread turns list handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/turns/list",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_turns_list_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_turns_list_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "cursor": null,
        "limit": 20,
        "sortDirection": "desc",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/turns/list",
        thread_turns_list_params.clone(),
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
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
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
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
            id: RequestId::String("thread-turns-list".to_string()),
            method: "thread/turns/list".to_string(),
            params: Some(thread_turns_list_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/turns/list should fail closed without replacement");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_turns_list_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread turns list handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/turns/list",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/turns/list, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_increment_elicitation_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_increment_elicitation_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let increment_response = serde_json::json!({
        "count": 2,
        "paused": true,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/increment_elicitation",
        thread_increment_elicitation_params.clone(),
        increment_response.clone(),
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
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
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

    let response = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-increment-elicitation".to_string()),
            method: "thread/increment_elicitation".to_string(),
            params: Some(thread_increment_elicitation_params),
            trace: None,
        },
    )
    .await
    .expect("thread/increment_elicitation should try a replacement worker")
    .expect("thread/increment_elicitation should restore from replacement worker");

    assert_eq!(response, increment_response);
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "thread_increment_elicitation_handoff_success")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread increment elicitation handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/increment_elicitation",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_increment_elicitation_records_handoff_failure_when_no_replacement_restores_context()
{
    let thread_increment_elicitation_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/increment_elicitation",
        thread_increment_elicitation_params.clone(),
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: "thread not found: thread-worker-b".to_string(),
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
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
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
            id: RequestId::String("thread-increment-elicitation".to_string()),
            method: "thread/increment_elicitation".to_string(),
            params: Some(thread_increment_elicitation_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/increment_elicitation should fail closed without replacement");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "thread_increment_elicitation_handoff_failure")],
    );
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread increment elicitation handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/increment_elicitation",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/increment_elicitation, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}
