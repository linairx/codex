use super::*;
use crate::northbound::v2_request_dispatch::handle_client_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_reconnects_missing_primary_worker_before_primary_worker_requests() {
    let cases = vec![
        (
            "configRequirements/read",
            None,
            serde_json::json!({
                "requirements": null,
            }),
        ),
        (
            "account/login/start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-reconnected",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            Some(serde_json::json!({
                "loginId": "login-reconnected",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "command/exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                "processId": "proc-reconnected",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "feedback/upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway reconnect regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-reconnected",
            }),
        ),
        (
            "account/sendAddCreditsNudgeEmail",
            Some(serde_json::json!({
                "creditType": "credits",
            })),
            serde_json::json!({
                "status": "sent",
            }),
        ),
        (
            "fuzzyFileSearch/sessionStart",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
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
        if method == "feedback/upload" {
            scope_registry.register_thread("thread-visible".to_string(), context.clone());
        }
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
        assert_eq!(router.worker_count(), 2);
        assert!(
            router.remove_worker(Some(0)),
            "test should drop the primary worker before reconnect"
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

        let result = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params: params.clone(),
                trace: None,
            },
        )
        .await
        .expect("primary-worker request should reach downstream workers")
        .expect("primary-worker request should succeed after reconnecting the primary worker");

        assert_eq!(router.worker_count(), 2);
        assert_eq!(result, expected_result);
        let worker_a_requests = worker_a_requests.lock().await.clone();
        if method == "configRequirements/read" {
            assert!(worker_a_requests.is_empty());
        } else {
            assert_eq!(worker_a_requests, vec![method.to_string()]);
        }
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    }
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_primary_worker_requests_during_reconnect_backoff()
{
    let (worker_a, worker_a_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "command/exec",
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        )
        .await;
    let worker_a_url = worker_a.clone();
    let (worker_b, worker_b_requests) =
        start_mock_remote_server_for_reconnectable_request_with_recording(
            "command/exec",
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        )
        .await;
    let worker_b_url = worker_b.clone();
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    let context = GatewayRequestContext {
        tenant_id: "tenant-primary".to_string(),
        project_id: Some("project-primary".to_string()),
    };
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
        router.remove_worker(Some(0)),
        "test should drop the primary worker before applying reconnect backoff"
    );
    router.record_worker_reconnect_failure(0, Instant::now(), Duration::from_secs(60));

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

    let logs = capture_logs_async(async {
        let err = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String("command-exec-request".to_string()),
                method: "command/exec".to_string(),
                params: Some(serde_json::json!({
                    "command": ["sh", "-lc", "printf gateway-reconnected-command"],
                    "processId": "proc-reconnected",
                    "tty": true,
                    "streamStdin": true,
                    "streamStdoutStderr": true,
                    "outputBytesCap": null,
                    "timeoutMs": null,
                    "cwd": null,
                    "env": null,
                    "size": {
                        "rows": 24,
                        "cols": 80,
                    },
                    "sandboxPolicy": null,
                })),
                trace: None,
            },
        )
        .await
        .expect_err("primary-worker request should not fall back during reconnect backoff");

        assert_eq!(
            err.to_string(),
            "primary worker route is unavailable for command/exec"
        );
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
    })
    .await;

    assert!(logs.contains(
        "gateway v2 request failed closed because required worker routes are unavailable"
    ));
    assert!(logs.contains("method=\"command/exec\""));
    assert!(logs.contains("tenant-primary"));
    assert!(logs.contains("project-primary"));
    assert!(logs.contains("available_worker_ids=[1]"));
    assert!(logs.contains(&format!(
        "available_worker_websocket_urls=[\"{worker_b_url}\"]"
    )));
    assert!(logs.contains("unavailable_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "unavailable_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_ids=[0]"));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_websocket_urls=[\"{worker_a_url}\"]"
    )));
    assert!(logs.contains("reconnect_backoff_worker_remaining_seconds=["));
    assert!(logs.contains(&format!(
        "reconnect_backoff_worker_routes=[(0, \"{worker_a_url}\", "
    )));
}

#[tokio::test]
async fn handle_client_request_does_not_fallback_primary_worker_method_family_during_reconnect_backoff()
 {
    let cases = vec![
        (
            "configRequirements/read",
            None,
            serde_json::json!({
                "requirements": [],
                "validationErrors": [],
            }),
        ),
        (
            "account/login/start",
            Some(serde_json::json!({
                "type": "chatgpt",
            })),
            serde_json::json!({
                "type": "chatgpt",
                "loginId": "login-primary",
                "authUrl": "https://example.com/login",
            }),
        ),
        (
            "account/login/cancel",
            Some(serde_json::json!({
                "loginId": "login-primary",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "account/sendAddCreditsNudgeEmail",
            Some(serde_json::json!({
                "creditType": "credits",
            })),
            serde_json::json!({
                "status": "sent",
            }),
        ),
        (
            "feedback/upload",
            Some(serde_json::json!({
                "classification": "bug",
                "reason": "gateway primary-worker backoff regression",
                "threadId": "thread-visible",
                "includeLogs": false,
                "extraLogFiles": [],
                "tags": {},
            })),
            serde_json::json!({
                "threadId": "feedback-thread-primary",
            }),
        ),
        (
            "command/exec",
            Some(serde_json::json!({
                "command": ["sh", "-lc", "printf gateway-primary"],
                "processId": "proc-primary",
                "tty": true,
                "streamStdin": true,
                "streamStdoutStderr": true,
                "outputBytesCap": null,
                "timeoutMs": null,
                "cwd": null,
                "env": null,
                "size": {
                    "rows": 24,
                    "cols": 80,
                },
                "sandboxPolicy": null,
            })),
            serde_json::json!({
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
            }),
        ),
        (
            "command/exec/write",
            Some(serde_json::json!({
                "processId": "proc-primary",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            Some(serde_json::json!({
                "processId": "proc-primary",
                "size": {
                    "rows": 40,
                    "cols": 120,
                },
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/terminate",
            Some(serde_json::json!({
                "processId": "proc-primary",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/readFile",
            Some(serde_json::json!({
                "path": "/tmp/project/input.txt",
            })),
            serde_json::json!({
                "dataBase64": "Z2F0ZXdheS1maWxl",
            }),
        ),
        (
            "fs/writeFile",
            Some(serde_json::json!({
                "path": "/tmp/project/output.txt",
                "dataBase64": "Z2F0ZXdheS13cml0ZQ==",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/createDirectory",
            Some(serde_json::json!({
                "path": "/tmp/project/nested",
                "recursive": true,
            })),
            serde_json::json!({}),
        ),
        (
            "fs/getMetadata",
            Some(serde_json::json!({
                "path": "/tmp/project/output.txt",
            })),
            serde_json::json!({
                "isDirectory": false,
                "isFile": true,
                "isSymlink": false,
                "createdAtMs": 0,
                "modifiedAtMs": 0,
            }),
        ),
        (
            "fs/readDirectory",
            Some(serde_json::json!({
                "path": "/tmp/project",
            })),
            serde_json::json!({
                "entries": [{
                    "fileName": "output.txt",
                    "isDirectory": false,
                    "isFile": true,
                }],
            }),
        ),
        (
            "fs/copy",
            Some(serde_json::json!({
                "sourcePath": "/tmp/project/output.txt",
                "destinationPath": "/tmp/project/copy.txt",
            })),
            serde_json::json!({}),
        ),
        (
            "fs/remove",
            Some(serde_json::json!({
                "path": "/tmp/project/copy.txt",
                "recursive": true,
                "force": true,
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
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
                expected_result,
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        let context = GatewayRequestContext::default();
        if method == "feedback/upload" {
            scope_registry.register_thread("thread-visible".to_string(), context.clone());
        }
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
            router.remove_worker(Some(0)),
            "test should drop the primary worker before applying reconnect backoff"
        );
        router.record_worker_reconnect_failure(0, Instant::now(), Duration::from_secs(60));

        let metrics = in_memory_metrics();
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

        let err = handle_client_request(
            &mut router,
            &connection,
            JSONRPCRequest {
                id: RequestId::String(format!("{method}-request")),
                method: method.to_string(),
                params,
                trace: None,
            },
        )
        .await
        .expect_err("primary-worker method should not fall back during reconnect backoff");

        if method == "fuzzyFileSearch" {
            assert_eq!(
                err.to_string(),
                "required worker routes are unavailable for fuzzyFileSearch: [0]"
            );
        } else {
            assert_eq!(
                err.to_string(),
                format!("primary worker route is unavailable for {method}")
            );
        }
        assert_eq!(router.worker_count(), 1);
        assert_eq!(*worker_a_requests.lock().await, Vec::<String>::new());
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());
        assert_v2_fail_closed_request_metric(&metrics, method, true);
    }
}
