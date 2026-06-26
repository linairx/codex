use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_scope_thread::response_thread_id;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn thread_id_resume_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_resume_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "history": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "personality": null,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/resume",
        thread_resume_params.clone(),
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

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(thread_resume_params),
                    trace: None,
                },
            )
            .await
            .expect_err("thread/resume should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("thread_resume_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_resume_handoff_failure")]);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_id_fork_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_fork_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/fork",
        thread_fork_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-forked-worker-a",
                "name": "Worker A forked thread",
                "cwd": "/tmp/worker-a",
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/worker-a",
            "instructionSources": [],
            "approvalPolicy": "on-request",
            "approvalsReviewer": "user",
            "sandbox": { "type": "dangerFullAccess" },
            "reasoningEffort": null,
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

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect("thread/fork should try a replacement worker")
            .expect("thread/fork should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        response_thread_id(&response),
        Some("thread-forked-worker-a")
    );
    assert_eq!(
        scope_registry.thread_worker_id("thread-forked-worker-a"),
        Some(0)
    );
    assert!(logs.contains("method=\"thread/fork\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "thread_fork_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_fork_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(0, [("thread_fork_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_fork_handoff_success")
    );
    assert_eq!(health.last_account_capacity_event_worker_id, Some(0));
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
        .expect("thread fork handoff success event should be published");
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
            "method": "thread/fork",
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
async fn thread_id_fork_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_fork_params = serde_json::json!({
        "threadId": "thread-worker-b",
        "path": null,
        "model": null,
        "modelProvider": null,
        "cwd": null,
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "sandbox": null,
        "config": null,
        "baseInstructions": null,
        "developerInstructions": null,
        "ephemeral": true,
        "persistExtendedHistory": false,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/fork",
        thread_fork_params.clone(),
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

    let mut error = None;
    let logs = capture_logs_async(async {
        error = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-fork".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect_err("thread/fork should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("thread_fork_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "thread_fork_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("thread_fork_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("thread_fork_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("thread_fork_handoff_failure")
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
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("thread fork handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/fork",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}
