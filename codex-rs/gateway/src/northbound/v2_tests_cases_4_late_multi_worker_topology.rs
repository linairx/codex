use super::*;
use crate::northbound::v2_connection::GatewayV2ReconnectState;
use crate::northbound::v2_request_dispatch::handle_client_request;
use crate::northbound::v2_routing::worker_for_notification;
use crate::northbound::v2_routing::worker_for_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn primary_worker_requests_stay_fail_closed_with_one_live_worker_in_multi_worker_topology() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let session_factory = GatewayV2SessionFactory::remote_multi(
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
    );
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(1),
            worker_websocket_url: None,
            request_handle,
        }],
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
            session_factory,
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

    let err = match worker_for_request(
        &mut router,
        &GatewayScopeRegistry::default(),
        &GatewayRequestContext::default(),
        &JSONRPCRequest {
            id: RequestId::String("command-exec".to_string()),
            method: "command/exec".to_string(),
            params: Some(serde_json::json!({
                "command": ["pwd"],
                "tty": false,
                "streamStdin": false,
                "streamStdoutStderr": false,
            })),
            trace: None,
        },
    ) {
        Ok(_) => panic!("primary-worker request should stay fail-closed"),
        Err(err) => err,
    };

    assert_eq!(
        err.to_string(),
        "primary worker route is unavailable for command/exec"
    );

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn thread_scoped_requests_stay_sticky_with_one_live_worker_in_multi_worker_topology() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let session_factory = GatewayV2SessionFactory::remote_multi(
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
    );
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(1),
            worker_websocket_url: None,
            request_handle,
        }],
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
            session_factory,
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
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );

    let worker = worker_for_request(
        &mut router,
        &scope_registry,
        &context,
        &JSONRPCRequest {
            id: RequestId::String("thread-read".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .expect("thread-scoped request should stay sticky to live worker");
    assert_eq!(worker.worker_id, Some(1));

    let worker = worker_for_notification(
        &mut router,
        &scope_registry,
        &JSONRPCNotification {
            method: "thread/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
            })),
        },
    )
    .expect("thread-scoped notification should stay sticky to live worker");
    assert_eq!(worker.worker_id, Some(1));

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn thread_scoped_requests_fail_closed_when_account_capacity_is_exhausted() {
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
            session_factory,
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
    let context = GatewayRequestContext {
        tenant_id: "tenant-a".to_string(),
        project_id: Some("project-a".to_string()),
    };
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
    );
    let metrics = in_memory_metrics();
    let (operator_events_tx, _) = broadcast::channel(32);
    let mut operator_events_rx = operator_events_tx.subscribe();
    let observability = GatewayObservability::new(Some(metrics.clone()), false)
        .with_operator_events(operator_events_tx);
    let admission = GatewayAdmissionController::default();
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

    let cases = [
        (
            "turn/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "prompt": "continue",
            }),
        ),
        (
            "turn/steer",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-1",
                "message": "adjust course",
            }),
        ),
        (
            "turn/interrupt",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "turnId": "turn-1",
            }),
        ),
        (
            "app/list",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "cursor": null,
                "limit": 20,
            }),
        ),
        (
            "thread/unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/compact/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/shellCommand",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "command": "cargo test",
            }),
        ),
        (
            "thread/backgroundTerminals/clean",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/realtime/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "thread/realtime/appendText",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "text": "hello",
            }),
        ),
        (
            "thread/realtime/appendAudio",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "audio": "AA==",
            }),
        ),
        (
            "thread/realtime/stop",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
        (
            "mcpServer/resource/read",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "server": "filesystem",
                "uri": "file:///tmp/notes.txt",
            }),
        ),
        (
            "mcpServer/tool/call",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "server": "filesystem",
                "tool": "read_file",
                "arguments": {},
            }),
        ),
        (
            "review/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
        ),
    ];
    for (method, params) in cases {
        let err = match handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(method.to_string()),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        {
            Ok(_) => {
                panic!("{method} should fail closed instead of moving active context")
            }
            Err(err) => err,
        };

        let uses_short_message = method.starts_with("turn/")
            || method == "app/list"
            || method == "review/start"
            || method == "thread/shellCommand"
            || method == "thread/backgroundTerminals/clean"
            || method.starts_with("thread/realtime/")
            || method.starts_with("mcpServer/");

        let expected_message = if uses_short_message {
            format!(
                "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}"
            )
        } else {
            format!(
                "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for {method}, and no replacement worker restored the context"
            )
        };
        assert_eq!(err.to_string(), expected_message);
        let handoff_event = operator_events_rx
            .recv()
            .await
            .expect("active thread handoff failure event should be published");
        assert_eq!(
            handoff_event.method,
            if uses_short_message {
                "gateway/accountActiveThreadHandoffFailed"
            } else {
                "gateway/accountThreadHandoffFailed"
            }
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
                "reason": expected_message,
            })
        );
    }
    let health = observability.v2_connection_health().snapshot();
    assert_eq!(
        health.account_capacity_event_counts,
        [
            ("active_thread_handoff_failure".to_string(), 13),
            ("thread_compact_start_handoff_failure".to_string(), 1),
            ("thread_unsubscribe_handoff_failure".to_string(), 1),
        ]
        .into()
    );
    assert_eq!(
        health
            .account_capacity_event_worker_counts
            .iter()
            .map(|counts| (counts.worker_id, counts.event_counts.clone()))
            .collect::<Vec<_>>(),
        vec![(
            1,
            [
                ("active_thread_handoff_failure".to_string(), 13),
                ("thread_compact_start_handoff_failure".to_string(), 1),
                ("thread_unsubscribe_handoff_failure".to_string(), 1),
            ]
            .into()
        )]
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
            "thread thread-worker-b is pinned to worker 1 with exhausted account capacity for review/start"
        )
    );
    assert_eq!(health.last_account_capacity_event_at.is_some(), true);
    assert_v2_account_capacity_event_metric_count(&metrics, 1, "active_thread_handoff_failure", 13);
    client_a
        .shutdown()
        .await
        .expect("client A should shut down");
    client_b
        .shutdown()
        .await
        .expect("client B should shut down");
}
