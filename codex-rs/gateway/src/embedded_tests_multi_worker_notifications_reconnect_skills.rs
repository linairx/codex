use super::*;

#[tokio::test]
async fn remote_multi_worker_deduplicates_skills_changed_notifications_after_worker_reconnect() {
    let worker_a = start_reconnecting_v2_multi_connection_skills_changed_server().await;
    let worker_b = start_mock_remote_multi_connection_skills_changed_server().await;
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

    let client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            let Some(remote_workers) = health.remote_workers.as_ref() else {
                panic!("remote workers should exist");
            };
            if health.status == GatewayHealthStatus::Ok
                && remote_workers[0].healthy
                && remote_workers[0].last_error.is_some()
                && remote_workers[1].healthy
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before v2 client connects");

    let mut v2_client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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
    .expect("v2 client should connect after worker reconnect");

    let first_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("initial skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        first_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate initial skills/changed should be suppressed after reconnect"
    );

    let _: SkillsListResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::SkillsList {
            request_id: RequestId::Integer(1),
            params: SkillsListParams {
                cwds: vec!["/tmp/shared-repo".into()],
                force_reload: true,
            },
        }),
    )
    .await
    .expect("skills/list should finish in time after reconnect")
    .expect("skills/list should succeed through multi-worker gateway after reconnect");

    let second_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));
    assert!(
        timeout(Duration::from_millis(200), v2_client.next_event())
            .await
            .is_err(),
        "duplicate post-refresh skills/changed should be suppressed after reconnect"
    );

    assert_remote_client_shutdown(v2_client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
