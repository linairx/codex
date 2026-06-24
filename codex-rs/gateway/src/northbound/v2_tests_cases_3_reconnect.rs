use super::*;
use crate::northbound::v2_request_dispatch::handle_client_request;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_aggregated_realtime_list_voices() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "thread/realtime/listVoices",
        serde_json::json!({
            "voices": {
                "v1": ["juniper", "maple"],
                "v2": ["alloy"],
                "defaultV1": "juniper",
                "defaultV2": "alloy",
            },
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "thread/realtime/listVoices",
        serde_json::json!({
            "voices": {
                "v1": ["maple", "cove"],
                "v2": ["alloy", "marin"],
                "defaultV1": "cove",
                "defaultV2": "marin",
            },
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
    assert_eq!(router.worker_count(), 2);
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker before reconnect"
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
            id: RequestId::String("realtime-list-voices".to_string()),
            method: "thread/realtime/listVoices".to_string(),
            params: Some(serde_json::json!({})),
            trace: None,
        },
    )
    .await
    .expect("thread/realtime/listVoices should reach downstream workers")
    .expect("thread/realtime/listVoices should succeed after reconnecting the missing worker");

    let response: ThreadRealtimeListVoicesResponse =
        serde_json::from_value(result).expect("thread/realtime/listVoices response should decode");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        response,
        ThreadRealtimeListVoicesResponse {
            voices: RealtimeVoicesList {
                v1: vec![
                    RealtimeVoice::Juniper,
                    RealtimeVoice::Maple,
                    RealtimeVoice::Cove,
                ],
                v2: vec![RealtimeVoice::Alloy, RealtimeVoice::Marin],
                default_v1: RealtimeVoice::Juniper,
                default_v2: RealtimeVoice::Alloy,
            },
        }
    );
}

#[tokio::test]
async fn handle_client_request_routes_multi_worker_config_read_by_matching_cwd() {
    let (worker_a_tx, worker_a_rx) = oneshot::channel();
    let worker_a = start_mock_remote_server_for_single_request(
        worker_a_tx,
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let (worker_b_tx, worker_b_rx) = oneshot::channel();
    let worker_b = start_mock_remote_server_for_single_request(
        worker_b_tx,
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
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

    let result = handle_client_request(
        &mut router,
        &connection,
        JSONRPCRequest {
            id: RequestId::String("config-read".to_string()),
            method: "config/read".to_string(),
            params: Some(serde_json::json!({
                "includeLayers": true,
                "cwd": "/tmp/worker-b/subdir",
            })),
            trace: None,
        },
    )
    .await
    .expect("config/read should reach downstream workers")
    .expect("config/read should succeed through matching worker");

    assert_eq!(
        result,
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        })
    );

    let worker_a_request = worker_a_rx.await.expect("worker A should capture request");
    let worker_b_request = worker_b_rx.await.expect("worker B should capture request");
    assert_eq!(worker_a_request.method, "config/read");
    assert_eq!(worker_b_request.method, "config/read");
}

#[tokio::test]
async fn handle_client_request_reconnects_missing_worker_before_config_read_by_matching_cwd() {
    let worker_a = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-a",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "user",
                    "file": "/tmp/worker-a/config.toml",
                },
                "version": "worker-a-config-version",
                "config": {
                    "model": "gpt-5-worker-a",
                },
                "disabledReason": null,
            }],
        }),
    )
    .await;
    let worker_b = start_mock_remote_server_for_reconnectable_request(
        "config/read",
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
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
    assert_eq!(router.worker_count(), 2);
    assert!(
        router.remove_worker(Some(1)),
        "test should drop the second worker before reconnect"
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
            id: RequestId::String("config-read".to_string()),
            method: "config/read".to_string(),
            params: Some(serde_json::json!({
                "includeLayers": true,
                "cwd": "/tmp/worker-b/subdir",
            })),
            trace: None,
        },
    )
    .await
    .expect("config/read should reach downstream workers")
    .expect("config/read should succeed after reconnecting the missing worker");

    assert_eq!(router.worker_count(), 2);
    assert_eq!(
        result,
        serde_json::json!({
            "config": {
                "model": "gpt-5-worker-b",
            },
            "origins": {},
            "layers": [{
                "name": {
                    "type": "project",
                    "dotCodexFolder": "/tmp/worker-b",
                },
                "version": "worker-b-config-version",
                "config": {
                    "model": "gpt-5-worker-b",
                },
                "disabledReason": null,
            }],
        })
    );
}
