use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_3_late_notifications.rs"]
mod v2_tests_cases_3_late_notifications;

#[tokio::test]
async fn websocket_upgrade_deduplicates_multi_worker_skills_changed_until_refresh() {
    let worker_a =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-a", vec!["skill-a"])
            .await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
            .await;
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
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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

    send_initialize(&mut websocket).await;

    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(Duration::from_secs(1), websocket.next())
            .await
            .expect("expected skills/list response or refreshed notification")
            .expect("websocket message should exist")
            .expect("websocket message should decode");
        let Message::Text(text) = message else {
            continue;
        };
        let parsed =
            serde_json::from_str::<JSONRPCMessage>(&text).expect("json-rpc message should decode");
        match parsed {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after skills/list refresh: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(post_refresh_duplicate.is_err(), true);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_keeps_multi_worker_skills_changed_pending_after_failed_refresh() {
    let worker_a = start_mock_remote_server_for_skills_changed_and_failing_list().await;
    let worker_b = start_mock_remote_server_for_idle_session().await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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

    send_initialize(&mut websocket).await;
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("failed-skills-list".to_string()),
                method: "skills/list".to_string(),
                params: Some(serde_json::json!({
                    "cwds": ["/tmp/worker-a"],
                    "forceReload": false,
                    "perCwdExtraUserRoots": null,
                })),
                trace: None,
            }))
            .expect("skills/list request should serialize")
            .into(),
        ))
        .await
        .expect("skills/list request should send");

    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "skills/changed", "pending_refresh");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_reopens_multi_worker_skills_changed_after_failed_then_successful_refresh()
 {
    let worker_a = start_mock_remote_server_for_skills_changed_failing_then_successful_list().await;
    let worker_b =
        start_mock_remote_server_for_skills_changed_and_list("/tmp/worker-b", vec!["skill-b"])
            .await;
    let metrics = in_memory_metrics();
    let (addr, server_task) = spawn_test_server(GatewayV2State {
        auth: GatewayAuth::Disabled,
        admission: GatewayAdmissionController::default(),
        observability: GatewayObservability::new(Some(metrics.clone()), false),
        scope_registry: Arc::new(GatewayScopeRegistry::default()),
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

    send_initialize(&mut websocket).await;
    assert_jsonrpc_notification(
        read_websocket_message(&mut websocket).await,
        "skills/changed",
        serde_json::json!({}),
    );

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("failed-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;
    let JSONRPCMessage::Error(error) = read_websocket_message(&mut websocket).await else {
        panic!("failed skills/list should return an error");
    };
    assert_eq!(
        error.id,
        RequestId::String("failed-skills-list".to_string())
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(duplicate.is_err());

    send_jsonrpc_request(
        &mut websocket,
        RequestId::String("successful-skills-list".to_string()),
        "skills/list",
        serde_json::json!({
            "cwds": ["/tmp/worker-a", "/tmp/worker-b"],
            "forceReload": false,
            "perCwdExtraUserRoots": null,
        }),
    )
    .await;

    let mut skills_changed_notifications = 0;
    let mut skills_list_response = None;
    for _ in 0..4 {
        let message = timeout(
            Duration::from_secs(1),
            read_websocket_message(&mut websocket),
        )
        .await
        .expect("skills/list response or refreshed notification should arrive");
        match message {
            JSONRPCMessage::Notification(notification) => {
                assert_eq!(notification.method, "skills/changed");
                assert_eq!(notification.params, Some(serde_json::json!({})));
                skills_changed_notifications += 1;
            }
            JSONRPCMessage::Response(response) => {
                if response.id == RequestId::String("successful-skills-list".to_string()) {
                    skills_list_response = Some(response.result);
                }
            }
            other => panic!("unexpected message after successful skills/list: {other:?}"),
        }
        if skills_list_response.is_some() && skills_changed_notifications == 1 {
            break;
        }
    }

    assert_eq!(
        skills_list_response,
        Some(serde_json::json!({
            "data": [
                {
                    "cwd": "/tmp/worker-a",
                    "skills": [{
                        "name": "skill-a",
                        "description": "skill-a description",
                        "path": "/tmp/worker-a/skill-a",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                },
                {
                    "cwd": "/tmp/worker-b",
                    "skills": [{
                        "name": "skill-b",
                        "description": "skill-b description",
                        "path": "/tmp/worker-b/skill-b",
                        "scope": "repo",
                        "enabled": true,
                    }],
                    "errors": [],
                }
            ]
        }))
    );
    assert_eq!(skills_changed_notifications, 1);

    let post_refresh_duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert!(post_refresh_duplicate.is_err());

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_routes_multi_worker_plugin_management_to_first_successful_worker() {
    let cases = vec![
        (
            "plugin/read",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "plugin": {
                    "marketplaceName": "demo-marketplace",
                    "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                    "summary": {
                        "id": "demo-plugin@local",
                        "name": "demo-plugin",
                        "source": {
                            "type": "local",
                            "path": "/tmp/project/plugins/demo-plugin",
                        },
                        "installed": false,
                        "enabled": false,
                        "installPolicy": "AVAILABLE",
                        "authPolicy": "ON_USE",
                        "interface": {
                            "displayName": "Demo Plugin",
                            "shortDescription": "Gateway passthrough plugin",
                            "longDescription": null,
                            "developerName": null,
                            "category": null,
                            "capabilities": [],
                            "websiteUrl": null,
                            "privacyPolicyUrl": null,
                            "termsOfServiceUrl": null,
                            "defaultPrompt": null,
                            "brandColor": null,
                            "composerIcon": null,
                            "composerIconUrl": null,
                            "logo": null,
                            "logoUrl": null,
                            "screenshots": [],
                            "screenshotUrls": [],
                        },
                    },
                    "description": "Gateway passthrough plugin description",
                    "skills": [],
                    "apps": [],
                    "mcpServers": [],
                },
            }),
        ),
        (
            "plugin/install",
            serde_json::json!({
                "marketplacePath": "/tmp/project/plugins/demo-marketplace.json",
                "remoteMarketplaceName": null,
                "pluginName": "demo-plugin",
            }),
            serde_json::json!({
                "authPolicy": "ON_USE",
                "appsNeedingAuth": [],
            }),
        ),
        (
            "plugin/uninstall",
            serde_json::json!({
                "pluginId": "demo-plugin@local",
            }),
            serde_json::json!({}),
        ),
        (
            "gitDiffToRemote",
            serde_json::json!({
                "cwd": "/tmp/worker-b/repo",
            }),
            serde_json::json!({
                "sha": "0123456789abcdef0123456789abcdef01234567",
                "diff": "diff --git a/README.md b/README.md\n",
            }),
        ),
    ];

    for (method, params, expected_result) in cases {
        let worker_a = start_mock_remote_server_for_passthrough_request_with_error(
            method,
            params.clone(),
            JSONRPCErrorError {
                code: crate::northbound::v2::INVALID_PARAMS_CODE,
                message: format!("{method} missing on worker-a"),
                data: None,
            },
        )
        .await;
        let worker_b = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            expected_result.clone(),
        )
        .await;
        let (addr, server_task) = spawn_test_server(GatewayV2State {
            auth: GatewayAuth::Disabled,
            admission: GatewayAdmissionController::default(),
            observability: GatewayObservability::default(),
            scope_registry: Arc::new(GatewayScopeRegistry::default()),
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

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(format!("{method}-request")),
                    method: method.to_string(),
                    params: Some(params.clone()),
                    trace: None,
                }))
                .expect("plugin management request should serialize")
                .into(),
            ))
            .await
            .expect("plugin management request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected plugin management response for {method}");
        };
        assert_eq!(response.id, RequestId::String(format!("{method}-request")));
        assert_eq!(response.result, expected_result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[path = "v2_tests_cases_3_late_reconnect.rs"]
mod v2_tests_cases_3_late_reconnect;
#[path = "v2_tests_cases_3_late_server_requests.rs"]
mod v2_tests_cases_3_late_server_requests;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_3_late_reconnect::*;
