use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_config_warning_notifications()
{
    let notification_params = serde_json::json!({
        "summary": "shared config warning",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "configWarning",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "configWarning",
        notification_params.clone(),
    )
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
        "configWarning",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "configWarning", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_deprecation_notice_notifications()
 {
    let notification_params = serde_json::json!({
        "summary": "shared deprecation notice",
        "details": null,
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "deprecationNotice",
        notification_params.clone(),
    )
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
        "deprecationNotice",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(&metrics, "deprecationNotice", "duplicate");

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn websocket_upgrade_deduplicates_exact_duplicate_multi_worker_external_agent_import_completed_notifications()
 {
    let notification_params = serde_json::json!({
        "importId": "import-1",
        "itemTypeResults": [],
    });
    let worker_a = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
        notification_params.clone(),
    )
    .await;
    let worker_b = start_mock_remote_server_for_connection_notification(
        "externalAgentConfig/import/completed",
        notification_params.clone(),
    )
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
        "externalAgentConfig/import/completed",
        notification_params,
    );

    let duplicate = timeout(Duration::from_millis(200), websocket.next()).await;
    assert_eq!(duplicate.is_err(), true);
    assert_v2_suppressed_notification_metric(
        &metrics,
        "externalAgentConfig/import/completed",
        "duplicate",
    );

    server_task.abort();
    let _ = server_task.await;
}
