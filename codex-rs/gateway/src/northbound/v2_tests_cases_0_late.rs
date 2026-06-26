use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_0_late_delivery.rs"]
mod v2_tests_cases_0_late_delivery;

#[path = "v2_tests_cases_0_late_connection_and_backlog.rs"]
mod v2_tests_cases_0_late_connection_and_backlog;

#[tokio::test]
async fn deliver_client_server_request_answer_fails_closed_when_account_is_exhausted() {
    let (client_a, request_handle_a) = start_test_request_handle().await;
    let (client_b, request_handle_b) = start_test_request_handle().await;
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (
            "ws://worker-a.invalid".to_string(),
            Some("acct-a".to_string()),
        ),
        (
            "ws://worker-b.invalid".to_string(),
            Some("acct-b".to_string()),
        ),
    ]));
    worker_health.mark_account_exhausted_for_worker(1, "quota reached".to_string());
    let session_factory = GatewayV2SessionFactory::remote_multi_with_account_ids(
        vec![
            RemoteAppServerConnectArgs {
                endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://worker-a.invalid".to_string(),
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
                    websocket_url: "ws://worker-b.invalid".to_string(),
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
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: vec![
            DownstreamWorkerHandle {
                worker_id: Some(0),
                worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
                request_handle: request_handle_a,
            },
            DownstreamWorkerHandle {
                worker_id: Some(1),
                worker_websocket_url: Some("ws://worker-b.invalid".to_string()),
                request_handle: request_handle_b,
            },
        ],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(super::super::super::GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory,
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: request_context.clone(),
            retry_backoff: Duration::from_secs(1),
        }),
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-worker-b".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Response(serde_json::json!({
                    "approved": true,
                })),
            )
            .await
            .expect_err("exhausted owner account should fail closed"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert_eq!(
        err.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
    );
    assert!(logs.contains("active_thread_handoff_failure"), "{logs}");
    assert!(logs.contains("response_kind=\"response\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-worker-b\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("exhausted_account_id=\"acct-b\""), "{logs}");
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "serverRequest/respond",
            "threadId": "thread-worker-b",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond",
        })
    );

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-error-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(1),
                    worker_websocket_url: test_worker_websocket_url(Some(1)),
                    downstream_request_id: RequestId::String(
                        "downstream-error-request".to_string(),
                    ),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-worker-b".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Error(JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                }),
            )
            .await
            .expect_err("exhausted owner account should fail closed for error replies"),
        );
    })
    .await;
    let err = err.expect("error reply delivery should fail");

    assert_eq!(
        err.to_string(),
        "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
    );
    assert!(logs.contains("active_thread_handoff_failure"), "{logs}");
    assert!(logs.contains("response_kind=\"error\""), "{logs}");
    assert!(logs.contains("thread_id=\"thread-worker-b\""), "{logs}");
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("exhausted_account_id=\"acct-b\""), "{logs}");
    assert_v2_server_request_answer_account_exhaustion_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "error", 1),
            ("client_server_request_answered", "response", 1),
            ("client_server_request_delivery_failed", "error", 1),
            ("client_server_request_delivery_failed", "response", 1),
        ],
        &[("error", 1), ("response", 1)],
        &[(1, "active_thread_handoff_failure", 2)],
    );
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("active_thread_handoff_failure".to_string(), 2)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("active_thread_handoff_failure".to_string(), 2)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("active_thread_handoff_failure")
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
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for serverRequest/respond"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("active thread handoff failure event should be published for error reply");
    assert_eq!(
        handoff_event.method,
        "gateway/accountActiveThreadHandoffFailed"
    );
    assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));

    client_a
        .shutdown()
        .await
        .expect("client A should shut down");
    client_b
        .shutdown()
        .await
        .expect("client B should shut down");
}

#[tokio::test]
async fn deliver_client_server_request_error_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext::default();
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        err = Some(
            super::super::super::deliver_client_server_request_answer(
                &downstream,
                &connection,
                RequestId::String("gateway-request".to_string()),
                super::super::super::PendingServerRequestRoute {
                    worker_id: Some(0),
                    worker_websocket_url: test_worker_websocket_url(Some(0)),
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                    method: "item/tool/requestUserInput".to_string(),
                    thread_id: Some("thread-visible".to_string()),
                },
                super::super::super::ClientServerRequestAnswer::Error(JSONRPCErrorError {
                    code: -32000,
                    message: "user rejected".to_string(),
                    data: None,
                }),
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains("failed to deliver answered server request back to downstream worker"),
        "{logs}"
    );
    assert!(logs.contains("response_kind=\"error\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(logs.contains("thread_id=\"thread-visible\""), "{logs}");
    assert_v2_server_request_lifecycle_and_answer_delivery_failure_metrics(
        &metrics,
        &[
            ("client_server_request_answered", "error", 1),
            ("client_server_request_delivery_failed", "error", 1),
        ],
        &[("error", 1)],
    );
}

#[tokio::test]
async fn reject_downstream_server_request_records_delivery_failure_lifecycle() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let admission = GatewayAdmissionController::default();
    let scope_registry = GatewayScopeRegistry::default();
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-visible".to_string(),
        project_id: Some("project-visible".to_string()),
    };
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(1),
        max_pending_server_requests: 1,
        max_pending_client_requests: 1,
        opt_out_notification_methods: HashSet::new(),
    };
    let (event_tx, event_rx) = mpsc::channel(1);
    let downstream = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: None,
    };

    let mut err = None;
    let logs = capture_logs_async(async {
        let gateway_request_id = RequestId::String("gateway-request".to_string());
        err = Some(
            super::super::super::reject_downstream_server_request_at_gateway_boundary(
                &downstream,
                &connection,
                super::super::super::GatewayRejectedServerRequest {
                    worker_id: Some(0),
                    worker_websocket_url: "ws://worker-a.invalid",
                    gateway_request_id: &gateway_request_id,
                    method: "item/tool/requestUserInput",
                    downstream_request_id: RequestId::String("downstream-request".to_string()),
                },
                JSONRPCErrorError {
                    code: super::super::super::INTERNAL_ERROR_CODE,
                    message: "gateway rejected prompt".to_string(),
                    data: None,
                },
            )
            .await
            .expect_err("missing downstream worker should fail delivery"),
        );
    })
    .await;
    let err = err.expect("delivery should fail");

    assert!(
        err.to_string()
            .contains("no downstream server-request route for worker Some(0)"),
        "unexpected error: {err}"
    );
    assert!(
        logs.contains(
            "failed to deliver gateway rejected server request back to downstream worker"
        ),
        "{logs}"
    );
    assert!(logs.contains("tenant_id=\"tenant-visible\""), "{logs}");
    assert!(logs.contains("project_id=\"project-visible\""), "{logs}");
    assert!(logs.contains("worker_id=Some(0)"), "{logs}");
    assert!(
        logs.contains("worker_websocket_url=\"ws://worker-a.invalid\""),
        "{logs}"
    );
    assert!(
        logs.contains("gateway_request_id=String(\"gateway-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("downstream_request_id=String(\"downstream-request\")"),
        "{logs}"
    );
    assert!(
        logs.contains("method=\"item/tool/requestUserInput\""),
        "{logs}"
    );
    assert_v2_server_request_lifecycle_and_rejection_delivery_failure_metrics(
        &metrics,
        &[(
            "downstream_server_request_rejection_delivery_failed",
            "item/tool/requestUserInput",
            1,
        )],
        "item/tool/requestUserInput",
        1,
    );
}

#[test]
fn server_request_backlog_worker_counts_groups_pending_and_answered_routes() {
    let pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-pending-a".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(2),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-a".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-b".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-b".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-c".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: None,
                worker_websocket_url: "embedded".to_string(),
                downstream_request_id: RequestId::String("downstream-pending-c".to_string()),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: None,
            },
        ),
    ]);
    let resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-a".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-a".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-b".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-b".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
    ]);

    assert_eq!(
        super::super::super::server_request_backlog_worker_counts(
            &pending_server_requests,
            &resolved_server_requests
        ),
        vec![
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: None,
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(1),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogWorkerCounts {
                worker_id: Some(2),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 2,
                server_request_backlog_count: 3,
            },
        ]
    );
}

#[test]
fn server_request_backlog_method_counts_groups_prompt_families() {
    let pending_server_requests = HashMap::from([
        (
            RequestId::String("gateway-pending-user-input".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(2),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-user-input".to_string(),
                ),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-elicitation".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: Some(1),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-elicitation".to_string(),
                ),
                method: "mcpServer/elicitation/request".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            RequestId::String("gateway-pending-permissions".to_string()),
            super::super::super::PendingServerRequestRoute {
                worker_id: None,
                worker_websocket_url: "embedded".to_string(),
                downstream_request_id: RequestId::String(
                    "downstream-pending-permissions".to_string(),
                ),
                method: "item/permissions/requestApproval".to_string(),
                thread_id: Some("thread-c".to_string()),
            },
        ),
    ]);
    let resolved_server_requests = HashMap::from([
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(2),
                request_id: RequestId::String("downstream-resolved-user-input".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-user-input".to_string()),
                worker_websocket_url: "ws://worker-c.invalid".to_string(),
                method: "item/tool/requestUserInput".to_string(),
                thread_id: Some("thread-a".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(1),
                request_id: RequestId::String("downstream-resolved-elicitation".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-elicitation".to_string()),
                worker_websocket_url: "ws://worker-b.invalid".to_string(),
                method: "mcpServer/elicitation/request".to_string(),
                thread_id: Some("thread-b".to_string()),
            },
        ),
        (
            super::super::super::DownstreamServerRequestKey {
                worker_id: Some(0),
                request_id: RequestId::String("downstream-resolved-refresh".to_string()),
            },
            super::super::super::ResolvedServerRequestRoute {
                gateway_request_id: RequestId::String("gateway-resolved-refresh".to_string()),
                worker_websocket_url: "ws://worker-a.invalid".to_string(),
                method: "account/chatgptAuthTokens/refresh".to_string(),
                thread_id: None,
            },
        ),
    ]);

    assert_eq!(
        super::super::super::server_request_backlog_method_counts(
            &pending_server_requests,
            &resolved_server_requests
        ),
        vec![
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "account/chatgptAuthTokens/refresh".to_string(),
                pending_server_request_count: 0,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/permissions/requestApproval".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 0,
                server_request_backlog_count: 1,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "item/tool/requestUserInput".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            },
            crate::api::GatewayV2ServerRequestBacklogMethodCounts {
                method: "mcpServer/elicitation/request".to_string(),
                pending_server_request_count: 1,
                answered_but_unresolved_server_request_count: 1,
                server_request_backlog_count: 2,
            },
        ]
    );
}

#[test]
fn server_request_resolved_notification_drops_duplicate_multi_worker_replays() {
    let gateway_request_id = RequestId::String("gateway-request-1".to_string());
    let downstream_request_id = RequestId::String("worker-request-1".to_string());
    let worker_id = Some(7);
    let mut resolved_server_requests = HashMap::from([(
        super::super::super::DownstreamServerRequestKey {
            worker_id,
            request_id: downstream_request_id.clone(),
        },
        super::super::super::ResolvedServerRequestRoute {
            gateway_request_id: gateway_request_id.clone(),
            worker_websocket_url: test_worker_websocket_url(worker_id),
            method: "item/tool/requestUserInput".to_string(),
            thread_id: Some("thread-worker-1".to_string()),
        },
    )]);
    let notification =
        ServerNotification::ServerRequestResolved(ServerRequestResolvedNotification {
            thread_id: "thread-worker-1".to_string(),
            request_id: downstream_request_id,
        });
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-visible".to_string(),
        project_id: Some("project-visible".to_string()),
    };
    let metrics = codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics");
    let observability = GatewayObservability::new(Some(metrics.clone()), false);

    let translated = super::super::super::server_notification_to_jsonrpc(
        notification.clone(),
        &request_context,
        &observability,
        worker_id,
        "ws://worker-b.invalid",
        &mut resolved_server_requests,
    )
    .expect("translated resolved notification should succeed");
    assert_eq!(
        translated,
        Some(JSONRPCNotification {
            method: "serverRequest/resolved".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-1",
                "requestId": gateway_request_id,
            })),
        })
    );
    assert_eq!(resolved_server_requests.is_empty(), true);

    let duplicate = super::super::super::server_notification_to_jsonrpc(
        notification,
        &request_context,
        &observability,
        worker_id,
        "ws://worker-b.invalid",
        &mut resolved_server_requests,
    )
    .expect("duplicate resolved notification should succeed");
    assert_eq!(duplicate, None);
    assert_v2_server_request_lifecycle_metrics(
        &metrics,
        &[
            (
                "downstream_server_request_resolved",
                "serverRequest/resolved",
                1,
            ),
            ("duplicate_resolved_replay", "serverRequest/resolved", 1),
        ],
    );
}

#[test]
fn record_project_worker_route_selected_updates_v2_connection_health_snapshot() {
    let observability = GatewayObservability::default();

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        Some("acct-b"),
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.project_worker_route_selection_count, 1);
    assert_eq!(
        health_snapshot.project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 1,
            }
        ]
    );
    assert_eq!(
        health_snapshot.last_project_worker_route_selected_worker_id,
        Some(7)
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_tenant_id
            .as_deref(),
        Some("tenant-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_project_id
            .as_deref(),
        Some("project-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_thread_id
            .as_deref(),
        Some("thread-a")
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_account_id
            .as_deref(),
        Some("acct-b")
    );
    assert!(
        health_snapshot
            .last_project_worker_route_selected_at
            .is_some()
    );
}

#[test]
fn record_project_worker_route_selected_tracks_missing_account_id_in_v2_connection_health_snapshot()
{
    let observability = GatewayObservability::default();

    observability.record_project_worker_route_selected(
        7,
        "tenant-a",
        "project-a",
        "thread-a",
        None,
    );

    let health_snapshot = observability.v2_connection_health().snapshot();
    assert_eq!(health_snapshot.project_worker_route_selection_count, 1);
    assert_eq!(
        health_snapshot.project_worker_route_selection_worker_counts,
        vec![
            crate::api::GatewayV2ProjectWorkerRouteSelectionWorkerCounts {
                worker_id: 7,
                project_worker_route_selection_count: 1,
            }
        ]
    );
    assert_eq!(
        health_snapshot
            .last_project_worker_route_selected_account_id
            .as_deref(),
        None
    );
    assert!(
        health_snapshot
            .last_project_worker_route_selected_at
            .is_some()
    );
}

#[path = "v2_tests_cases_0_late_connection_log.rs"]
mod v2_tests_cases_0_late_connection_log;

#[path = "v2_tests_cases_0_late_support.rs"]
mod v2_tests_cases_0_late_support;

pub(crate) use self::v2_tests_cases_0_late_support::*;

#[path = "v2_tests_cases_0_late_server_request_support.rs"]
mod v2_tests_cases_0_late_server_request_support;

pub(crate) use self::v2_tests_cases_0_late_server_request_support::*;

#[path = "v2_tests_cases_0_late_metrics_support.rs"]
mod v2_tests_cases_0_late_metrics_support;

pub(crate) use self::v2_tests_cases_0_late_metrics_support::*;

pub(crate) fn in_memory_metrics() -> codex_otel::MetricsClient {
    codex_otel::MetricsClient::new(
        codex_otel::MetricsConfig::in_memory(
            "test",
            "codex-gateway",
            env!("CARGO_PKG_VERSION"),
            opentelemetry_sdk::metrics::InMemoryMetricExporter::default(),
        )
        .with_runtime_reader(),
    )
    .expect("metrics")
}

#[test]
fn observe_v2_request_records_rate_limited_metrics_and_audit_log() {
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), true);
    let context = GatewayRequestContext {
        tenant_id: "tenant-quota".to_string(),
        project_id: Some("project-quota".to_string()),
    };

    let logs = capture_logs(|| {
        super::super::super::observe_v2_request(
            &observability,
            &context,
            "turn/start",
            "rate_limited",
            Duration::from_millis(7),
        );
    });

    assert!(logs.contains("codex_gateway.audit"));
    assert!(logs.contains("gateway v2 request completed"));
    assert!(logs.contains("method=\"turn/start\""));
    assert!(logs.contains("outcome=\"rate_limited\""));
    assert!(logs.contains("tenant_id=\"tenant-quota\""));
    assert!(logs.contains("project_id=\"project-quota\""));
    assert_v2_request_metrics(&metrics, &[("turn/start", "rate_limited", 1)]);
}
