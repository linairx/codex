use super::*;

use crate::northbound::v2_connection::GatewayV2ReconnectState;

#[path = "v2_tests_cases_4_late_connection_cleanup.rs"]
mod v2_tests_cases_4_late_connection_cleanup;

#[path = "v2_tests_cases_4_late_cleanup_duplicate.rs"]
mod v2_tests_cases_4_late_cleanup_duplicate;

#[path = "v2_tests_cases_4_late_cleanup_single_pending.rs"]
mod v2_tests_cases_4_late_cleanup_single_pending;

#[path = "v2_tests_cases_4_late_cleanup_single_unresolved.rs"]
mod v2_tests_cases_4_late_cleanup_single_unresolved;

#[path = "v2_tests_cases_4_late_cleanup_worker_disconnect.rs"]
mod v2_tests_cases_4_late_cleanup_worker_disconnect;

#[path = "v2_tests_cases_4_late_cleanup_worker_session.rs"]
mod v2_tests_cases_4_late_cleanup_worker_session;

#[tokio::test]
async fn reconnectable_router_keeps_multi_worker_topology_with_one_live_worker() {
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
    let reconnect_state = GatewayV2ReconnectState {
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
    };
    let router = GatewayV2DownstreamRouter {
        workers: Vec::new(),
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: false,
        active_fs_watches: HashMap::new(),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(reconnect_state),
    };

    assert!(router.multi_worker_topology());
}
