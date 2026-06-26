use super::*;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_scope_thread::response_thread_id;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn thread_read_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_read_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "includeTurns": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_c = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker C restored thread",
                "cwd": "/tmp/worker-c",
            },
        }),
    )
    .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
        (worker_c.clone(), Some("acct-c".to_string())),
    ]));
    worker_health.mark_unhealthy(0, Some("socket closed".to_string()));
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
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: worker_c,
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
        vec![
            Some("acct-a".to_string()),
            Some("acct-b".to_string()),
            Some("acct-c".to_string()),
        ],
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
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(thread_read_params),
            trace: None,
        },
    )
    .await
    .expect("thread/read should try a replacement worker")
    .expect("thread/read should restore from replacement worker");

    assert_eq!(response_thread_id(&response), Some("thread-worker-b"));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(2));
    assert_v2_account_capacity_event_metrics(&metrics, &[(2, "thread_read_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_read_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(2, [("thread_read_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_read_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(2));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some("thread id request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread read handoff success event should be published");
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
            "method": "thread/read",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-c",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_read_rejects_replacement_worker_returning_wrong_thread() {
    let thread_read_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "includeTurns": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/read",
        thread_read_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "cwd": "/tmp/worker-a",
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
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(thread_read_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/read should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_read_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_read_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("thread_read_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_read_handoff_failure")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(1));
    assert_eq!(
        health.last_account_capacity_event_tenant_id.as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health.last_account_capacity_event_project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health.last_account_capacity_event_reason.as_deref(),
        Some(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread read handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/read",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/read, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_unarchive_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_unarchive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/unarchive",
        thread_unarchive_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
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
            id: RequestId::String("thread-unarchive".to_string()),
            method: "thread/unarchive".to_string(),
            params: Some(thread_unarchive_params),
            trace: None,
        },
    )
    .await
    .expect("thread/unarchive should try a replacement worker")
    .expect("thread/unarchive should restore from replacement worker");

    assert_eq!(response_thread_id(&response), Some("thread-worker-b"));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_unarchive_handoff_success")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread unarchive handoff success event should be published");
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
            "method": "thread/unarchive",
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
async fn thread_unarchive_rejects_replacement_worker_returning_wrong_thread() {
    let thread_unarchive_params = serde_json::json!({
        "threadId": "thread-worker-b",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/unarchive",
        thread_unarchive_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong worker thread",
                "cwd": "/tmp/worker-a",
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
            id: RequestId::String("thread-unarchive".to_string()),
            method: "thread/unarchive".to_string(),
            params: Some(thread_unarchive_params),
            trace: None,
        },
    )
    .await
    .expect_err("thread/unarchive should fail closed when replacement returns wrong thread");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_unarchive_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread unarchive handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/unarchive",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/unarchive, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}
