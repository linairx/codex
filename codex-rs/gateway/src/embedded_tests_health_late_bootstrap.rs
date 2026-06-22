use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_resolves_pending_thread_scoped_server_requests_when_worker_disconnects()
{
    let worker_a =
        start_mock_remote_multi_connection_disconnect_after_server_request_server().await;
    let worker_b =
        start_mock_remote_multi_connection_thread_server("thread-worker-b", "/tmp/worker-b").await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_a,

                        auth_token: None,
                        account_id: None,
                    },
                    GatewayRemoteWorkerConfig {
                        websocket_url: worker_b,

                        auth_token: None,
                        account_id: None,
                    },
                ],
            }),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let mut client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
        endpoint: codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
            websocket_url: format!("ws://{}/", server.local_addr()),
            auth_token: None,
        },
        client_name: "codex-gateway-test".to_string(),
        client_version: "0.0.0-test".to_string(),
        experimental_api: true,
        mcp_server_openai_form_elicitation: false,
        opt_out_notification_methods: Vec::new(),
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-a".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("first thread/start should succeed through multi-worker remote gateway");
    assert_eq!(started.thread.id, "thread-worker-a");

    let server_request_id = timeout(Duration::from_secs(5), async {
        match client
            .next_event()
            .await
            .expect("event stream should stay open")
        {
            AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                request_id,
                params,
            }) => {
                assert_eq!(params.thread_id, "thread-worker-a");
                assert_eq!(params.turn_id, "turn-worker-a");
                assert_eq!(params.item_id, "tool-call-worker-a");
                request_id
            }
            AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(_)) => {
                panic!("serverRequest/resolved should follow the server request")
            }
            other => panic!("unexpected event: {other:?}"),
        }
    })
    .await
    .expect("thread-scoped server request should arrive");

    timeout(Duration::from_secs(5), async {
        match client
            .next_event()
            .await
            .expect("event stream should stay open")
        {
            AppServerEvent::ServerNotification(ServerNotification::ServerRequestResolved(
                notification,
            )) => {
                assert_eq!(notification.thread_id, "thread-worker-a");
                assert_eq!(notification.request_id, server_request_id);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    })
    .await
    .expect("gateway should resolve the disconnected worker's pending request");

    let second_started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(2),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/project-b".to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(true),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("second thread/start should still succeed after one worker disconnects");
    assert_eq!(second_started.thread.id, "thread-worker-b");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
