use super::*;

use crate::northbound::v2_connection::GatewayV2ReconnectState;

#[tokio::test]
async fn reconnect_missing_workers_logs_attempt_and_success_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_b = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
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
            worker_websocket_urls: vec!["ws://worker-a.invalid".to_string(), worker_b.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
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
                            websocket_url: worker_b.clone(),
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
            ),
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

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(
        logs.contains("attempting to reconnect missing downstream worker session"),
        "{logs}"
    );
    assert!(
        logs.contains("reconnected missing downstream worker session"),
        "{logs}"
    );
    assert!(logs.contains("worker_id=1"), "{logs}");
    assert!(
        logs.contains(format!("websocket_url=\"{worker_b}\"").as_str()),
        "{logs}"
    );
    assert!(
        logs.contains("initialized_notification_sent=false"),
        "{logs}"
    );
    assert!(logs.contains("active_fs_watch_count=0"), "{logs}");
    assert!(logs.contains("retry_backoff_seconds=1"), "{logs}");

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_logs_connect_failure_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_a = start_mock_remote_server_for_initialize().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: Some("ws://worker-a.invalid".to_string()),
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![worker_a, "ws://127.0.0.1:1".to_string()],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://127.0.0.1:9".to_string(),
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
                            websocket_url: "ws://127.0.0.1:1".to_string(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(7),
        }),
    };

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("attempting to reconnect missing downstream worker session"));
    assert!(logs.contains("failed to reconnect missing downstream worker session"));
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains("websocket_url=\"ws://127.0.0.1:1\""));
    assert!(logs.contains("initialized_notification_sent=true"));
    assert!(logs.contains("active_fs_watch_count=1"));
    assert!(logs.contains("retry_backoff_seconds=7"));

    client.shutdown().await.expect("client should shut down");
}

#[tokio::test]
async fn reconnect_missing_workers_logs_replay_failure_context() {
    let (client, request_handle) = start_test_request_handle().await;
    let (event_tx, event_rx) = mpsc::channel(1);
    let worker_a = start_mock_remote_server_for_initialize().await;
    let replay_failure_worker =
        start_mock_remote_server_for_reconnectable_initialized_replay_failure().await;
    let mut router = GatewayV2DownstreamRouter {
        workers: vec![DownstreamWorkerHandle {
            worker_id: Some(0),
            worker_websocket_url: None,
            request_handle,
        }],
        event_tx,
        event_rx,
        shutdown_txs: Vec::new(),
        event_tasks: Vec::new(),
        next_worker: 0,
        initialized_notification_sent: true,
        active_fs_watches: HashMap::from([(
            "watch-shared".to_string(),
            FsWatchParams {
                watch_id: "watch-shared".to_string(),
                path: PathBuf::from("/tmp/shared/project/.git/HEAD")
                    .try_into()
                    .expect("fs watch path should be absolute"),
            },
        )]),
        reconnect_retry_after: HashMap::new(),
        reconnect_state: Some(GatewayV2ReconnectState {
            configured_worker_ids: vec![0, 1],
            worker_websocket_urls: vec![worker_a, replay_failure_worker.clone()],
            session_factory: GatewayV2SessionFactory::remote_multi(
                vec![
                    RemoteAppServerConnectArgs {
                        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                            websocket_url: "ws://127.0.0.1:9".to_string(),
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
                            websocket_url: replay_failure_worker.clone(),
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
            ),
            initialize_params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-tui".to_string(),
                    title: None,
                    version: "0.0.0-test".to_string(),
                },
                capabilities: None,
            },
            request_context: GatewayRequestContext::default(),
            retry_backoff: Duration::from_secs(5),
        }),
    };

    let logs = capture_logs_async(async move {
        router
            .reconnect_missing_workers_at(Instant::now(), &GatewayObservability::default(), true)
            .await;
        timeout(Duration::from_secs(2), router.shutdown())
            .await
            .expect("router shutdown should finish in time")
            .expect("router should shut down");
    })
    .await;

    assert!(logs.contains("attempting to reconnect missing downstream worker session"));
    assert!(
        logs.contains("failed to replay connection state to reconnected downstream worker session")
    );
    assert!(logs.contains("worker_id=1"));
    assert!(logs.contains(format!("websocket_url=\"{replay_failure_worker}\"").as_str()));
    assert!(logs.contains("initialized_notification_sent=true"));
    assert!(logs.contains("active_fs_watch_count=1"));
    assert!(logs.contains("retry_backoff_seconds=5"));

    client.shutdown().await.expect("client should shut down");
}
