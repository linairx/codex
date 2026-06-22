use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_supports_server_request_roundtrip_over_v2() {
    let websocket_url = start_mock_remote_server_for_server_request_roundtrip().await;
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
    .expect("remote client should connect to remote gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some("/tmp/remote-project".to_string()),
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
        .expect("thread/start should succeed through remote gateway");
    assert_eq!(started.thread.id, "thread-remote-workflow");

    let (request_id, params) = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::ToolRequestUserInput {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("server request should arrive");
    assert_eq!(params.thread_id, "thread-remote-workflow");
    assert_eq!(params.turn_id, "turn-remote-workflow");
    assert_eq!(params.item_id, "tool-call-remote-workflow");
    assert_eq!(params.questions.len(), 1);
    assert_eq!(params.questions[0].id, "mode");

    let mut answers = HashMap::new();
    answers.insert(
        "mode".to_string(),
        ToolRequestUserInputAnswer {
            answers: vec!["safe".to_string()],
        },
    );
    client
        .resolve_server_request(
            request_id,
            serde_json::to_value(ToolRequestUserInputResponse { answers })
                .expect("server request response should serialize"),
        )
        .await
        .expect("server request should resolve");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
