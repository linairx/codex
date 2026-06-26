use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;

#[tokio::test]
async fn conversation_summary_by_id_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": thread_id,
                "path": "/tmp/worker-a/rollout.jsonl",
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
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

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
                    id: RequestId::String("conversation-summary-id".to_string()),
                    method: "getConversationSummary".to_string(),
                    params: Some(summary_params),
                    trace: None,
                },
            )
            .await
            .expect("getConversationSummary by id should try a replacement worker")
            .expect("getConversationSummary by id should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(
        response["summary"]["conversationId"],
        serde_json::json!(thread_id)
    );
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(0));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-a/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"getConversationSummary\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=0"), "{logs}");
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "conversation_summary_handoff_success")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            0,
            [("conversation_summary_handoff_success".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_success")
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
    assert!(health.last_account_capacity_event_at.is_some());
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff success event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountThreadHandoffSucceeded"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 0,
            "replacementAccountId": "acct-a",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_by_id_rejects_replacement_worker_returning_wrong_thread() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let wrong_thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c9";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "getConversationSummary",
        summary_params.clone(),
        serde_json::json!({
            "summary": {
                "conversationId": wrong_thread_id,
                "path": "/tmp/worker-a/rollout.jsonl",
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
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

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
            id: RequestId::String("conversation-summary-id".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary by id should fail closed on wrong conversation id");

    let expected_message = format!(
        "thread {thread_id} is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(error.to_string(), expected_message);
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(1));
    assert_eq!(scope_registry.thread_worker_id(wrong_thread_id), None);
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-a/rollout.jsonl"),
        None
    );
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "conversation_summary_handoff_failure")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [("conversation_summary_handoff_failure".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_failure")
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
        Some(expected_message.as_str())
    );
    assert!(health.last_account_capacity_event_at.is_some());
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": expected_message,
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn conversation_summary_by_id_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
    let summary_params = serde_json::json!({
        "conversationId": thread_id,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "getConversationSummary",
        summary_params.clone(),
        JSONRPCErrorError {
            code: INVALID_PARAMS_CODE,
            message: format!("thread not found: {thread_id}"),
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
    scope_registry.register_thread_with_worker(thread_id.to_string(), context.clone(), Some(1));

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
            id: RequestId::String("conversation-summary-id".to_string()),
            method: "getConversationSummary".to_string(),
            params: Some(summary_params),
            trace: None,
        },
    )
    .await
    .expect_err("getConversationSummary by id should fail closed");

    let expected_message = format!(
        "thread {thread_id} is pinned to worker 1 with exhausted account capacity for getConversationSummary, and no replacement worker restored the context"
    );
    assert_eq!(error.to_string(), expected_message);
    assert_eq!(scope_registry.thread_worker_id(thread_id), Some(1));
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(1, "conversation_summary_handoff_failure")],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("conversation_summary_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [("conversation_summary_handoff_failure".to_string(), 1)].into()
        )]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("conversation_summary_handoff_failure")
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
        Some(expected_message.as_str())
    );
    assert!(health.last_account_capacity_event_at.is_some());
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("conversation summary handoff failure event should be published");
    assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
    assert_eq!(handoff_event.thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "getConversationSummary",
            "threadId": thread_id,
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": expected_message,
        })
    );

    router.shutdown().await.expect("router shutdown");
}
