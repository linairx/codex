use crate::admission::GatewayAdmissionController;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2::tests::cases::assert_v2_account_capacity_event_metrics;
use crate::northbound::v2::tests::cases::capture_logs_async;
use crate::northbound::v2::tests::cases::in_memory_metrics;
use crate::northbound::v2::tests::cases::start_mock_remote_server_for_idle_session;
use crate::northbound::v2::tests::cases::start_mock_remote_server_for_passthrough_request_with_error;
use crate::northbound::v2::tests::cases::start_mock_remote_server_for_passthrough_request_with_result;
use crate::northbound::v2::tests::cases::test_initialize_response;
use crate::northbound::v2_connection::GatewayV2ConnectionContext;
use crate::northbound::v2_connection::GatewayV2DownstreamRouter;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_scope_thread::response_thread_id;
use crate::observability::GatewayObservability;
use crate::remote_health::RemoteWorkerHealthRegistry;
use crate::scope::GatewayRequestContext;
use crate::scope::GatewayScopeRegistry;
use crate::v2::GatewayV2SessionFactory;
use codex_app_server_client::RemoteAppServerConnectArgs;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
use pretty_assertions::assert_eq;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[tokio::test]
async fn handle_client_request_routes_path_based_thread_resume_by_visible_path() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/worker-b/rollout.jsonl",
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
            message: "thread not found: /tmp/worker-b/rollout.jsonl".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
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
    let admission = GatewayAdmissionController::default();
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

    let result = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-resume-path".to_string()),
            method: "thread/resume".to_string(),
            params: Some(serde_json::json!({
                "threadId": "ignored-thread-id",
                "path": "/tmp/worker-b/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect("path-based thread/resume should reach downstream workers")
    .expect("path-based thread/resume should succeed through visible path discovery");

    assert_eq!(
        result,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-b",
                "name": "Worker B thread",
                "cwd": "/tmp/worker-b",
                "path": "/tmp/worker-b/rollout.jsonl",
            },
        })
    );
    assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/worker-b/rollout.jsonl"),
        Some(1)
    );
}

#[tokio::test]
async fn path_based_thread_resume_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
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
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Worker A restored thread",
                "cwd": "/tmp/worker-a",
                "path": "/tmp/shared/rollout.jsonl",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let worker_c = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params.clone(),
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Worker C restored thread",
                "cwd": "/tmp/worker-c",
                "path": "/tmp/shared/rollout.jsonl",
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

    let mut response = None;
    let logs = capture_logs_async(async {
        response = Some(
            handle_client_request(
                &mut router,
                &connection,
                JSONRPCRequest {
                    id: RequestId::String("thread-resume-path".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "ignored-thread-id",
                        "path": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect("path-based thread/resume should try a replacement worker")
            .expect("path-based thread/resume should restore from replacement worker"),
        );
    })
    .await;
    let response = response.expect("response should be captured");

    assert_eq!(response_thread_id(&response), Some("thread-worker-a"));
    assert_eq!(scope_registry.thread_worker_id("thread-worker-a"), Some(2));
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(2)
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("replacement_worker_id=2"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(2, "path_thread_handoff_success")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_success".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(2, [("path_thread_handoff_success".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_success")
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
        Some("path-based thread request restored on a replacement account-backed worker")
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    let handoff_event = operator_events_rx
        .recv()
        .await
        .expect("path handoff success event should be published");
    assert_eq!(handoff_event.method, "gateway/accountPathHandoffSucceeded");
    assert_eq!(
        handoff_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "replacementWorkerId": 2,
            "replacementAccountId": "acct-c",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_resume_rejects_replacement_worker_returning_wrong_path() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
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
    let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
        "thread/resume",
        thread_resume_params,
        serde_json::json!({
            "thread": {
                "id": "thread-worker-a",
                "name": "Wrong restored path",
                "cwd": "/tmp/worker-a",
                "path": "/tmp/other/rollout.jsonl",
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
            id: RequestId::String("thread-resume-path".to_string()),
            method: "thread/resume".to_string(),
            params: Some(serde_json::json!({
                "threadId": "ignored-thread-id",
                "path": "/tmp/shared/rollout.jsonl",
            })),
            trace: None,
        },
    )
    .await
    .expect_err("path-based thread/resume should fail closed on wrong restored path");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
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
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("path_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_failure")
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
            "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
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
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn path_based_thread_resume_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_resume_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
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
        thread_resume_params,
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
                    id: RequestId::String("thread-resume-path".to_string()),
                    method: "thread/resume".to_string(),
                    params: Some(serde_json::json!({
                        "threadId": "ignored-thread-id",
                        "path": "/tmp/shared/rollout.jsonl",
                    })),
                    trace: None,
                },
            )
            .await
            .expect_err("path-based thread/resume should fail closed without replacement"),
        );
    })
    .await;
    let error = error.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [("path_thread_handoff_failure".to_string(), 1)].into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(1, [("path_thread_handoff_failure".to_string(), 1)].into())]
    );
    assert_eq!(
        health.last_account_capacity_event.as_deref(),
        Some("path_thread_handoff_failure")
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
            "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
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
            "method": "thread/resume",
            "threadPath": "/tmp/shared/rollout.jsonl",
            "exhaustedWorkerId": 1,
            "exhaustedAccountId": "acct-b",
            "reason": "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/resume, and no replacement worker restored the context",
        })
    );

    router.shutdown().await.expect("router shutdown");
}
