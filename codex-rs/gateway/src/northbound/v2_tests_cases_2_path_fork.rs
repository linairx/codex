use crate::admission::GatewayAdmissionController;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2::tests::cases::assert_v2_account_capacity_event_health;
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

#[tokio::test]
async fn path_based_thread_fork_uses_replacement_worker_when_pinned_account_is_exhausted() {
    let thread_fork_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
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
                "path": "/tmp/shared/rollout.jsonl",
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
                    id: RequestId::String("thread-fork-path".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect("path-based thread/fork should try a replacement worker")
            .expect("path-based thread/fork should restore from replacement worker"),
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
    assert_eq!(
        scope_registry.thread_path_worker_id("/tmp/shared/rollout.jsonl"),
        Some(0)
    );
    assert!(logs.contains("method=\"thread/fork\""), "{logs}");
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
async fn path_based_thread_fork_records_handoff_failure_when_no_replacement_restores_context() {
    let thread_fork_params = serde_json::json!({
        "threadId": "ignored-thread-id",
        "path": "/tmp/shared/rollout.jsonl",
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
                    id: RequestId::String("thread-fork-path".to_string()),
                    method: "thread/fork".to_string(),
                    params: Some(thread_fork_params),
                    trace: None,
                },
            )
            .await
            .expect_err("path-based thread/fork should fail closed without replacement"),
        );
    })
    .await;
    let error = response.expect("error should be captured");

    assert_eq!(
        error.to_string(),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context"
    );
    assert!(logs.contains("exhausted_worker_id=1"), "{logs}");
    assert!(logs.contains("path_thread_handoff_failure"), "{logs}");
    assert_v2_account_capacity_event_metrics(&metrics, &[(1, "path_thread_handoff_failure")]);
    assert_v2_account_capacity_event_health(
        &observability,
        1,
        "path_thread_handoff_failure",
        1,
        "tenant-a",
        Some("project-a"),
        "thread path /tmp/shared/rollout.jsonl is pinned to worker 1 with exhausted account capacity for thread/fork, and no replacement worker restored the context",
    );

    router.shutdown().await.expect("router shutdown");
}
