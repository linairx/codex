use super::*;
use crate::northbound::v2::INVALID_PARAMS_CODE;
use crate::northbound::v2_request_dispatch::handle_client_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn additional_thread_id_controls_use_replacement_worker_when_pinned_account_is_exhausted() {
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            serde_json::json!({}),
            "thread_name_set_handoff_success",
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            serde_json::json!({}),
            "thread_memory_mode_set_handoff_success",
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 1,
                "paused": false,
            }),
            "thread_decrement_elicitation_handoff_success",
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            }),
            serde_json::json!({}),
            "thread_inject_items_handoff_success",
        ),
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
            "thread_unsubscribe_handoff_success",
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
            "thread_compact_start_handoff_success",
        ),
    ];

    for (method, request_id, params, expected_response, success_metric) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            expected_response.clone(),
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
                id: RequestId::String(request_id.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .unwrap_or_else(|_| panic!("{method} should try a replacement worker"))
        .unwrap_or_else(|_| panic!("{method} should restore from replacement worker"));

        assert_eq!(response, expected_response);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(0));
        assert_v2_account_capacity_event_metrics(&metrics, &[(0, success_metric)]);
        let health = observability.v2_connection_health().snapshot();
        assert_eq!(
            health.account_capacity_event_counts,
            [(success_metric.to_string(), 1)].into()
        );
        assert_eq!(
            health
                .account_capacity_event_worker_counts
                .iter()
                .map(|counts| (counts.worker_id, counts.event_counts.clone()))
                .collect::<Vec<_>>(),
            vec![(0, [(success_metric.to_string(), 1)].into())]
        );
        assert_eq!(
            health.last_account_capacity_event.as_deref(),
            Some(success_metric)
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
            .unwrap_or_else(|_| panic!("{method} handoff success event should be published"));
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
                "method": method,
                "threadId": "thread-worker-b",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "replacementWorkerId": 0,
                "replacementAccountId": "acct-a",
            })
        );

        router.shutdown().await.expect("router shutdown");
    }
}

#[tokio::test]
async fn additional_thread_id_controls_record_handoff_failure_when_no_replacement_restores_context()
{
    let cases = vec![
        (
            "thread/name/set",
            "thread-name-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
            "thread_name_set_handoff_failure",
        ),
        (
            "thread/memoryMode/set",
            "thread-memory-mode-set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
            "thread_memory_mode_set_handoff_failure",
        ),
        (
            "thread/decrement_elicitation",
            "thread-decrement-elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_decrement_elicitation_handoff_failure",
        ),
        (
            "thread/inject_items",
            "thread-inject-items",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "Injected reply",
                        "annotations": [],
                    }],
                }],
            }),
            "thread_inject_items_handoff_failure",
        ),
        (
            "thread/unsubscribe",
            "thread-unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_unsubscribe_handoff_failure",
        ),
        (
            "thread/compact/start",
            "thread-compact-start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            "thread_compact_start_handoff_failure",
        ),
    ];

    for (method, request_id, params, failure_metric) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
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
                id: RequestId::String(request_id.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .unwrap_err();

        let expected_message = format!(
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}, and no replacement worker restored the context"
        );
        assert_eq!(error.to_string(), expected_message);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
        assert_v2_account_capacity_event_metrics(&metrics, &[(1, failure_metric)]);
        let health = observability.v2_connection_health().snapshot();
        assert_eq!(
            health.account_capacity_event_counts,
            [(failure_metric.to_string(), 1)].into()
        );
        assert_eq!(
            health
                .account_capacity_event_worker_counts
                .iter()
                .map(|counts| (counts.worker_id, counts.event_counts.clone()))
                .collect::<Vec<_>>(),
            vec![(1, [(failure_metric.to_string(), 1)].into())]
        );
        assert_eq!(
            health.last_account_capacity_event.as_deref(),
            Some(failure_metric)
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
        assert_eq!(health.last_account_capacity_event_at.is_some(), true);
        let handoff_event = operator_events_rx
            .recv()
            .await
            .unwrap_or_else(|_| panic!("{method} handoff failure event should be published"));
        assert_eq!(handoff_event.method, "gateway/accountThreadHandoffFailed");
        assert_eq!(handoff_event.thread_id.as_deref(), Some("thread-worker-b"));
        assert_eq!(
            handoff_event.data,
            serde_json::json!({
                "tenantId": "tenant-a",
                "projectId": "project-a",
                "method": method,
                "threadId": "thread-worker-b",
                "exhaustedWorkerId": 1,
                "exhaustedAccountId": "acct-b",
                "reason": expected_message,
            })
        );

        router.shutdown().await.expect("router shutdown");
    }
}
