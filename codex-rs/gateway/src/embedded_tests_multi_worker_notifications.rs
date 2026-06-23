use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_forwards_skills_changed_notifications_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_single_worker_skills_changed_server().await;
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
    let events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
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
        channel_capacity: 16,
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
    .expect("skills/list should succeed through remote gateway after reconnect");

    let second_notification = timeout(Duration::from_secs(5), v2_client.next_event())
        .await
        .expect("post-refresh skills/changed should finish in time after reconnect")
        .expect("event stream should stay open after reconnect");
    assert!(matches!(
        second_notification,
        AppServerEvent::ServerNotification(ServerNotification::SkillsChanged(_))
    ));

    assert_remote_client_shutdown(v2_client.shutdown().await);
    drop(events_response);
    server.shutdown().await.expect("shutdown");
}
