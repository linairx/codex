use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_routes_git_diff_to_first_successful_worker() {
    let git_diff_params = serde_json::json!({
        "cwd": "/tmp/worker-b/repo",
    });
    let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
        "gitDiffToRemote",
        git_diff_params.clone(),
        JSONRPCErrorError {
            code: crate::northbound::v2::INVALID_PARAMS_CODE,
            message: "failed to compute git diff to remote for cwd: /tmp/worker-b/repo".to_string(),
            data: None,
        },
    )
    .await;
    let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
        "gitDiffToRemote",
        git_diff_params,
        serde_json::json!({
            "sha": "0123456789abcdef0123456789abcdef01234567",
            "diff": "diff --git a/README.md b/README.md\n",
        }),
    )
    .await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
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
    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
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

    let result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("git-diff-to-remote".to_string()),
            method: "gitDiffToRemote".to_string(),
            params: Some(serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            })),
            trace: None,
        },
    )
    .await
    .expect("gitDiffToRemote should reach downstream workers")
    .expect("gitDiffToRemote should succeed through worker discovery");

    assert_eq!(
        result,
        serde_json::json!({
            "sha": "0123456789abcdef0123456789abcdef01234567",
            "diff": "diff --git a/README.md b/README.md\n",
        })
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_sticky_thread_mutations() {
    let cases = vec![
        (
            "thread/name/set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "name": "Renamed thread",
            }),
        ),
        (
            "thread/memoryMode/set",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "mode": "enabled",
            }),
        ),
    ];

    for (method, params) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                serde_json::json!({}),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                serde_json::json!({}),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
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
        assert!(
            router.remove_worker(Some(1)),
            "test should drop the sticky worker before reconnect"
        );
        assert_eq!(router.worker_count(), 1);

        let admission = GatewayAdmissionController::default();
        let observability = GatewayObservability::default();
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

        let result = crate::northbound::v2::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .expect("thread mutation should reach downstream workers")
        .expect("thread mutation should succeed after reconnecting the sticky worker");

        assert_eq!(result, serde_json::json!({}));
        assert_eq!(router.worker_count(), 2);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    }
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_additional_sticky_thread_controls()
{
    let cases = vec![
        (
            "thread/unsubscribe",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "status": "unsubscribed",
            }),
        ),
        (
            "thread/archive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/unarchive",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
            }),
        ),
        (
            "thread/metadata/update",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "gitInfo": {
                    "sha": "abc123",
                    "branch": "main",
                    "originUrl": null,
                },
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "gitInfo": {
                        "commitHash": "abc123",
                        "branchName": "main",
                        "remoteUrl": null,
                    },
                },
            }),
        ),
        (
            "thread/turns/list",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "cursor": null,
                "limit": 20,
                "sortDirection": "desc",
            }),
            serde_json::json!({
                "data": [{
                    "id": "turn-1",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": 1,
                    "completedAt": 2,
                    "durationMs": 1,
                }],
                "nextCursor": null,
                "backwardsCursor": null,
            }),
        ),
        (
            "thread/increment_elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 2,
                "paused": true,
            }),
        ),
        (
            "thread/decrement_elicitation",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({
                "count": 1,
                "paused": false,
            }),
        ),
        (
            "thread/inject_items",
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
        ),
        (
            "thread/compact/start",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/shellCommand",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "command": "git status --short",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/backgroundTerminals/clean",
            serde_json::json!({
                "threadId": "thread-worker-b",
            }),
            serde_json::json!({}),
        ),
        (
            "thread/rollback",
            serde_json::json!({
                "threadId": "thread-worker-b",
                "numTurns": 2,
            }),
            serde_json::json!({
                "thread": {
                    "id": "thread-worker-b",
                    "name": "Recovered thread",
                    "turns": [],
                },
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        scope_registry.register_thread_with_worker(
            "thread-worker-b".to_string(),
            context.clone(),
            Some(1),
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
        assert!(
            router.remove_worker(Some(1)),
            "test should drop the sticky worker before reconnect"
        );
        assert_eq!(router.worker_count(), 1);

        let admission = GatewayAdmissionController::default();
        let observability = GatewayObservability::default();
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

        let result = crate::northbound::v2::handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params: Some(params),
                trace: None,
            },
        )
        .await
        .expect("thread control should reach downstream workers")
        .expect("thread control should succeed after reconnecting the sticky worker");

        assert_eq!(result, expected_result);
        assert_eq!(router.worker_count(), 2);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, vec![method.to_string()]);
        assert_eq!(scope_registry.thread_worker_id("thread-worker-b"), Some(1));
    }
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_thread_fork_and_backfills_new_thread_route()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_reconnectable_thread_fork_and_read().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
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
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the sticky worker before reconnect"
    );

    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
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

    let fork_result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-fork".to_string()),
            method: "thread/fork".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "path": null,
                "model": "gpt-5",
                "modelProvider": null,
                "cwd": "/tmp/worker-b",
                "approvalPolicy": null,
                "approvalsReviewer": null,
                "sandbox": null,
                "config": null,
                "baseInstructions": null,
                "developerInstructions": null,
                "ephemeral": true,
                "persistExtendedHistory": false,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread/fork should reach downstream workers")
    .expect("thread/fork should succeed after reconnecting the sticky worker");

    assert_eq!(
        fork_result,
        serde_json::json!({
            "thread": {
                "id": "thread-forked",
                "name": "Forked thread",
                "cwd": "/tmp/worker-b",
            },
            "model": "gpt-5",
            "modelProvider": "openai",
            "serviceTier": null,
            "cwd": "/tmp/worker-b",
            "instructionSources": [],
            "approvalPolicy": "on-request",
            "approvalsReviewer": "user",
            "sandbox": { "type": "dangerFullAccess" },
            "reasoningEffort": null,
        })
    );
    assert_eq!(router.worker_count(), 2);
    assert_eq!(scope_registry.thread_worker_id("thread-forked"), Some(1));

    let thread_read_result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read-forked".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-forked",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread/read should reach downstream workers")
    .expect("thread/read should stay sticky to the recovered worker");

    assert_eq!(
        thread_read_result,
        serde_json::json!({
            "thread": {
                "id": "thread-forked",
                "name": "Forked thread",
                "cwd": "/tmp/worker-b",
            },
        })
    );
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_review_start_and_backfills_review_thread_route()
 {
    let worker_a = start_mock_remote_server_for_initialize().await;
    let worker_b = start_mock_remote_server_for_reconnectable_review_start_then_thread_read().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext::default();
    scope_registry.register_thread_with_worker(
        "thread-worker-b".to_string(),
        context.clone(),
        Some(1),
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
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the sticky worker before reconnect"
    );

    let admission = GatewayAdmissionController::default();
    let observability = GatewayObservability::default();
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

    let review_result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("review-start".to_string()),
            method: "review/start".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-worker-b",
                "target": {
                    "type": "custom",
                    "instructions": "Review the current change",
                },
                "delivery": "detached",
            })),
            trace: None,
        },
    )
    .await
    .expect("review/start should reach downstream workers")
    .expect("review/start should succeed after reconnecting the sticky worker");

    assert_eq!(
        review_result,
        serde_json::json!({
            "turn": {
                "id": "turn-review",
                "items": [],
                "status": "pending",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null,
            },
            "reviewThreadId": "thread-review",
        })
    );
    assert_eq!(router.worker_count(), 2);
    assert_eq!(scope_registry.thread_worker_id("thread-review"), Some(1));

    let thread_read_result = crate::northbound::v2::handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("thread-read-review".to_string()),
            method: "thread/read".to_string(),
            params: Some(serde_json::json!({
                "threadId": "thread-review",
                "includeTurns": false,
            })),
            trace: None,
        },
    )
    .await
    .expect("thread/read should reach downstream workers")
    .expect("thread/read should stay sticky to the recovered review worker");

    assert_eq!(
        thread_read_result,
        serde_json::json!({
            "thread": {
                "id": "thread-review",
                "name": "Detached review thread",
            },
        })
    );
}
