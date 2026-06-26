use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_reconnects_missing_primary_worker_before_primary_worker_requests() {
    let cases = vec![
        (
            "configRequirements/read",
            "config-requirements-read",
            None,
            serde_json::json!({
                "requirements": [],
                "validationErrors": [],
            }),
        ),
        (
            "account/login/start",
            "account-login-start",
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
            "account-login-cancel",
            Some(serde_json::json!({
                "loginId": "login-reconnected",
            })),
            serde_json::json!({
                "status": "canceled",
            }),
        ),
        (
            "command/exec",
            "command-exec",
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
            "command-exec-write",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
                "deltaBase64": "AQID",
            })),
            serde_json::json!({}),
        ),
        (
            "command/exec/resize",
            "command-exec-resize",
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
            "command-exec-terminate",
            Some(serde_json::json!({
                "processId": "proc-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "feedback/upload",
            "feedback-upload",
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
            "fuzzyFileSearch/sessionStart",
            "fuzzy-file-search-session-start",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "roots": ["/tmp/project"],
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionUpdate",
            "fuzzy-file-search-session-update",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
                "query": "gate",
            })),
            serde_json::json!({}),
        ),
        (
            "fuzzyFileSearch/sessionStop",
            "fuzzy-file-search-session-stop",
            Some(serde_json::json!({
                "sessionId": "search-session-reconnected",
            })),
            serde_json::json!({}),
        ),
        (
            "windowsSandbox/setupStart",
            "windows-sandbox-setup-start",
            Some(serde_json::json!({
                "mode": "unelevated",
                "cwd": "/tmp/project",
            })),
            serde_json::json!({
                "started": true,
            }),
        ),
    ];

    for (method, request_id, params, expected_result) in cases {
        let (worker_a, worker_a_requests) =
            start_mock_remote_server_for_disconnect_then_reconnectable_request_with_recording(
                method,
                expected_result.clone(),
            )
            .await;
        let (worker_b, worker_b_requests) =
            start_mock_remote_server_for_reconnectable_request_with_recording(
                method,
                serde_json::json!({
                    "worker": "unexpected-secondary",
                }),
            )
            .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        if let Some(thread_id) = params
            .as_ref()
            .and_then(|value| value.get("threadId"))
            .and_then(Value::as_str)
        {
            scope_registry.register_thread(thread_id.to_string(), GatewayRequestContext::default());
        }
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry,
            session_factory: Some(Arc::new(GatewayV2SessionFactory::remote_multi(
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
            ))),
            timeouts: GatewayV2Timeouts::default(),
        })
        .await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize_with_capabilities(
            &mut websocket,
            Some(InitializeCapabilities {
                request_attestation: false,
                experimental_api: true,
                mcp_server_openai_form_elicitation: false,
                opt_out_notification_methods: None,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: params.clone(),
                    trace: None,
                }))
                .expect("primary-worker request should serialize")
                .into(),
            ))
            .await
            .expect("primary-worker request should send");

        let JSONRPCMessage::Response(response) = timeout(
            Duration::from_secs(2),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("primary-worker response should arrive") else {
            panic!("expected primary-worker response for {method}");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, expected_result);
        assert_eq!(*worker_a_requests.lock().await, vec![method.to_string()]);
        assert_eq!(*worker_b_requests.lock().await, Vec::<String>::new());

        server_task.abort();
        let _ = server_task.await;
    }
}
