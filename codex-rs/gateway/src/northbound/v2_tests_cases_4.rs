use super::*;
use pretty_assertions::assert_eq;

#[path = "v2_tests_cases_4_notifications.rs"]
mod v2_tests_cases_4_notifications;

#[path = "v2_tests_cases_4_realtime.rs"]
mod v2_tests_cases_4_realtime;

#[path = "v2_tests_cases_4_thread_notifications.rs"]
mod v2_tests_cases_4_thread_notifications;

#[path = "v2_tests_cases_4_realtime_notifications.rs"]
mod v2_tests_cases_4_realtime_notifications;

#[path = "v2_tests_cases_4_late.rs"]
mod v2_tests_cases_4_late;

#[path = "v2_tests_cases_4_reconnect_logs.rs"]
mod v2_tests_cases_4_reconnect_logs;

#[path = "v2_tests_cases_4_reconnect_metrics.rs"]
mod v2_tests_cases_4_reconnect_metrics;

#[path = "v2_tests_cases_4_thread_start.rs"]
mod v2_tests_cases_4_thread_start;

#[path = "v2_tests_cases_4_thread_start_tail_account_capacity.rs"]
mod v2_tests_cases_4_thread_start_tail_account_capacity;

#[path = "v2_tests_cases_4_thread_start_tail.rs"]
mod v2_tests_cases_4_thread_start_tail;

#[allow(unused_imports)]
pub(crate) use self::v2_tests_cases_4_late::*;

#[path = "v2_tests_cases_4_reconnect_and_fs.rs"]
mod v2_tests_cases_4_reconnect_and_fs;

#[tokio::test]
async fn downstream_router_emits_disconnect_only_once_per_worker_session() {
    let worker_a =
        start_mock_remote_server_for_reconnectable_initialized_and_fs_watch_replay().await;
    let worker_b = start_mock_remote_server_for_initialize().await;
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

    let disconnect_event = timeout(Duration::from_secs(2), router.next_event())
        .await
        .expect("disconnect event should finish in time")
        .expect("disconnect event should exist");
    assert_eq!(disconnect_event.worker_id, Some(0));
    let Some(AppServerEvent::Disconnected { message }) = disconnect_event.event else {
        panic!("expected a terminal disconnect event from the first worker");
    };
    assert_eq!(message.is_empty(), false);

    assert!(
        timeout(Duration::from_millis(200), router.next_event())
            .await
            .is_err(),
        "terminal disconnect should not be followed by a duplicate worker end-of-stream event"
    );

    timeout(Duration::from_secs(2), router.shutdown())
        .await
        .expect("router shutdown should finish in time")
        .expect("router should shut down");
}

#[path = "v2_tests_cases_4_late_multi_worker_topology.rs"]
mod v2_tests_cases_4_late_multi_worker_topology;
