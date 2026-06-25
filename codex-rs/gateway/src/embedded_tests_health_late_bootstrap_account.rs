use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_aggregates_bootstrap_account_and_models_over_v2() {
    let worker_a = start_mock_remote_multi_connection_bootstrap_server(
        OpenAiAuthRequirement::NotRequired,
        Some(vec![("codex", "Codex", 20), ("shared", "Shared", 5)]),
        vec![
            ("shared-model", "Shared Model", true),
            ("worker-a-model", "Worker A Model", false),
        ],
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_bootstrap_server(
        OpenAiAuthRequirement::Required,
        Some(vec![("codex", "Codex", 20), ("worker-b", "Worker B", 35)]),
        vec![
            ("shared-model", "Shared Model", true),
            ("worker-b-model", "Worker B Model", false),
        ],
    )
    .await;
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

    let client = RemoteAppServerClient::connect(RemoteAppServerConnectArgs {
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

    let account: GetAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        }),
    )
    .await
    .expect("account/read should finish in time")
    .expect("account/read should aggregate through multi-worker gateway");
    assert_eq!(account.account, None);
    assert_eq!(account.requires_openai_auth, true);

    let rate_limits: GetAccountRateLimitsResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::GetAccountRateLimits {
            request_id: RequestId::Integer(2),
            params: None,
        }),
    )
    .await
    .expect("account/rateLimits/read should finish in time")
    .expect("account/rateLimits/read should aggregate through multi-worker gateway");
    assert_eq!(rate_limits.rate_limits.limit_id.as_deref(), Some("codex"));
    assert_eq!(
        rate_limits
            .rate_limits_by_limit_id
            .expect("aggregated rate limits should include per-limit map")
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        HashSet::from(["codex", "shared", "worker-b"])
    );

    let models: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(3),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list should finish in time")
    .expect("model/list should aggregate through multi-worker gateway");
    assert_eq!(models.next_cursor, None);
    assert_eq!(
        models
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model", "worker-a-model", "worker-b-model"]
    );
    assert_eq!(
        models
            .data
            .iter()
            .filter(|model| model.is_default)
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model"]
    );

    let first_model_page: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(5),
            params: ModelListParams {
                cursor: None,
                limit: Some(2),
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list first page should finish in time")
    .expect("model/list first page should aggregate through multi-worker gateway");
    assert_eq!(
        first_model_page.next_cursor.as_deref(),
        Some("model-offset:2")
    );
    assert_eq!(
        first_model_page
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["shared-model", "worker-a-model"]
    );

    let second_model_page: ModelListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(6),
            params: ModelListParams {
                cursor: first_model_page.next_cursor,
                limit: Some(2),
                include_hidden: Some(true),
            },
        }),
    )
    .await
    .expect("model/list second page should finish in time")
    .expect("model/list second page should aggregate through multi-worker gateway");
    assert_eq!(second_model_page.next_cursor, None);
    assert_eq!(
        second_model_page
            .data
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["worker-b-model"]
    );

    assert_remote_client_shutdown(
        timeout(Duration::from_secs(5), client.shutdown())
            .await
            .expect("client shutdown should finish in time"),
    );
    timeout(Duration::from_secs(5), server.shutdown())
        .await
        .expect("server shutdown should finish in time")
        .expect("shutdown");
}
