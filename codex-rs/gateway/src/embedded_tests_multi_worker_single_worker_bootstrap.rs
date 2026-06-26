use super::*;

#[path = "embedded_tests_multi_worker_single_worker_bootstrap_discovery.rs"]
mod embedded_tests_multi_worker_single_worker_bootstrap_discovery;

#[path = "embedded_tests_multi_worker_single_worker_bootstrap_setup.rs"]
mod embedded_tests_multi_worker_single_worker_bootstrap_setup;

#[path = "embedded_tests_multi_worker_single_worker_bootstrap_tail.rs"]
mod embedded_tests_multi_worker_single_worker_bootstrap_tail;

#[tokio::test]
async fn remote_single_worker_supports_bootstrap_refresh_requests_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_bootstrap_refresh_server().await;
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");
    let server = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url,
                    auth_token: None,
                    account_id: None,
                }],
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
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && !worker.reconnecting)
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .and_then(|worker| worker.last_error.as_ref())
                    .is_some()
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

    embedded_tests_multi_worker_single_worker_bootstrap_discovery::assert_bootstrap_refresh_discovery_requests(
        &mut v2_client,
    )
    .await;
    embedded_tests_multi_worker_single_worker_bootstrap_setup::assert_bootstrap_refresh_setup_requests(
        &mut v2_client,
    )
    .await;
    embedded_tests_multi_worker_single_worker_bootstrap_tail::assert_bootstrap_refresh_tail_requests(
        &mut v2_client,
    )
    .await;

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), v2_client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
