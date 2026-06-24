use super::*;
use crate::northbound::v2_connection::GatewayV2ReconnectState;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_routing::worker_for_request;
use crate::northbound::v2_scope_thread::response_thread_id;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn thread_start_fails_closed_when_no_healthy_workers_have_capacity() {
    let (client_a, request_handle_a) = start_test_request_handle().await;
    let (client_b, request_handle_b) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
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
    worker_health.mark_unhealthy(0, Some("socket closed".to_string()));
    worker_health.mark_unhealthy(1, Some("socket closed".to_string()));
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![
            DownstreamWorkerHandle {
                worker_id: Some(0),
                worker_websocket_url: None,
                request_handle: request_handle_a,
            },
            DownstreamWorkerHandle {
                worker_id: Some(1),
                worker_websocket_url: None,
                request_handle: request_handle_b,
            },
        ],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![
                "ws://worker-a.invalid".to_string(),
                "ws://worker-b.invalid".to_string(),
            ],
            session_factory: GatewayV2SessionFactory::remote_multi_with_account_ids(
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
            .with_worker_health(worker_health),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(1),
        }),
    };
    let scope_registry = GatewayScopeRegistry::default();
    let project_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };

    let error = match worker_for_request(
        &mut router,
        &scope_registry,
        &project_context,
        &JSONRPCRequest {
            id: RequestId::String("thread-start".to_string()),
            method: "thread/start".to_string(),
            params: Some(serde_json::json!({
                "cwd": "/tmp/project-a",
            })),
            trace: None,
        },
    ) {
        Ok(worker) => panic!(
            "thread/start should fail closed when no healthy workers are available, but selected worker {:?}",
            worker.worker_id
        ),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("no healthy downstream app-server sessions with available account capacity"),
        "{error}"
    );
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
async fn thread_start_marks_exhausted_account_and_retries_available_account() {
    let params = serde_json::json!({
        "cwd": "/tmp/project-a",
    });
    let serialized_params = serde_json::json!({
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "baseInstructions": null,
        "config": null,
        "cwd": "/tmp/project-a",
        "developerInstructions": null,
        "dynamicTools": null,
        "ephemeral": null,
        "experimentalRawEvents": false,
        "mockExperimentalField": null,
        "model": null,
        "modelProvider": null,
        "persistExtendedHistory": false,
        "personality": null,
        "sandbox": null,
        "serviceName": null,
        "sessionStartSource": null,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/start",
        serialized_params.clone(),
        JSONRPCErrorError {
            code: 429,
            message: "rate limit reached".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b =
        start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
            "thread/start",
            Some(serialized_params),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp/project-a",
                "instructionSources": [],
                "approvalPolicy": "never",
                "approvalsReviewer": "user",
                "sandbox": {
                    "mode": "read-only",
                },
                "reasoningEffort": null,
            }),
        )
        .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
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
    .with_worker_health(worker_health.clone());
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &request_context)
            .await
            .expect("router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(4);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
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
                    id: RequestId::String("thread-start".to_string()),
                    method: "thread/start".to_string(),
                    params: Some(params),
                    trace: None,
                },
            )
            .await
            .expect("thread/start should retry another account")
            .expect("thread/start should succeed"),
        );
    })
    .await;
    let response = response.expect("thread/start response should be captured");

    assert_eq!(response_thread_id(&response), Some("thread-worker-b"));
    assert!(!worker_health.account_has_capacity(0));
    assert!(worker_health.account_has_capacity(1));
    assert!(logs.contains("exhausted_worker_id=0"), "{logs}");
    assert!(logs.contains("replacement_worker_id=1"), "{logs}");
    assert_v2_account_capacity_event_metrics(
        &metrics,
        &[(0, "exhausted"), (1, "thread_start_failover_success")],
    );
    let exhausted_event = operator_events_rx
        .recv()
        .await
        .expect("v2 account exhaustion event should be published");
    assert_eq!(exhausted_event.method, "gateway/accountCapacityExhausted");
    assert_eq!(
        exhausted_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "workerId": 0,
            "accountId": "acct-a",
            "reason": "rate limit reached",
        })
    );
    let failover_event = operator_events_rx
        .recv()
        .await
        .expect("v2 account failover event should be published");
    assert_eq!(failover_event.method, "gateway/accountFailoverSucceeded");
    assert_eq!(
        failover_event.data,
        serde_json::json!({
            "tenantId": "tenant-a",
            "projectId": "project-a",
            "replacementWorkerId": 1,
            "replacementAccountId": "acct-b",
            "exhaustedWorkerIds": [0],
        })
    );

    router.shutdown().await.expect("router shutdown");
}

#[tokio::test]
async fn thread_start_marks_exhausted_account_and_returns_first_capacity_error_without_replacement()
{
    let params = serde_json::json!({
        "cwd": "/tmp/project-a",
    });
    let serialized_params = serde_json::json!({
        "approvalPolicy": null,
        "approvalsReviewer": null,
        "baseInstructions": null,
        "config": null,
        "cwd": "/tmp/project-a",
        "developerInstructions": null,
        "dynamicTools": null,
        "ephemeral": null,
        "experimentalRawEvents": false,
        "mockExperimentalField": null,
        "model": null,
        "modelProvider": null,
        "persistExtendedHistory": false,
        "personality": null,
        "sandbox": null,
        "serviceName": null,
        "sessionStartSource": null,
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/start",
        serialized_params.clone(),
        JSONRPCErrorError {
            code: 429,
            message: "rate limit reached".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_error(
        "thread/start",
        serialized_params,
        JSONRPCErrorError {
            code: -32000,
            message: "billing quota reached".to_string(),
            data: None,
        },
    )
    .await;
    let worker_health = Arc::new(RemoteWorkerHealthRegistry::new_with_accounts(vec![
        (worker_a.clone(), Some("acct-a".to_string())),
        (worker_b.clone(), Some("acct-b".to_string())),
    ]));
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
    .with_worker_health(worker_health.clone());
    let initialize_params = InitializeParams {
        client_info: ClientInfo {
            name: "codex-tui".to_string(),
            title: None,
            version: "0.0.0-test".to_string(),
        },
        capabilities: None,
    };
    let request_context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    let mut router =
        GatewayV2DownstreamRouter::connect(&session_factory, &initialize_params, &request_context)
            .await
            .expect("router should connect");
    let admission = GatewayAdmissionController::default();
    let metrics = in_memory_metrics();
    let observability = GatewayObservability::new(Some(metrics.clone()), false);
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let connection = GatewayV2ConnectionContext {
        admission: &admission,
        observability: &observability,
        scope_registry: &scope_registry,
        request_context: &request_context,
        client_send_timeout: Duration::from_secs(10),
        max_pending_server_requests: 4,
        max_pending_client_requests: 4,
        opt_out_notification_methods: HashSet::new(),
    };

    let response = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-start".to_string()),
            method: "thread/start".to_string(),
            params: Some(params),
            trace: None,
        },
    )
    .await
    .expect("thread/start should exhaust every account")
    .expect_err("thread/start should return the first capacity error");

    assert_eq!(response.code, 429);
    assert_eq!(response.message, "rate limit reached");
    assert!(!worker_health.account_has_capacity(0));
    assert!(!worker_health.account_has_capacity(1));
    assert_v2_account_capacity_event_metrics(&metrics, &[(0, "exhausted"), (1, "exhausted")]);

    router.shutdown().await.expect("router shutdown");
}
