use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_turn_control_routing_and_notification_fan_in() {
    let worker_a = start_mock_remote_multi_connection_turn_control_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "worker a steer delta",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_turn_control_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "worker b steer delta",
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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let first_started: AppServerThreadStartResponse = client
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
        .expect("second thread/start should succeed through multi-worker remote gateway");

    let first_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start worker a".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("first turn/start should route to worker A");
    assert_eq!(first_turn_started.turn.id, "turn-thread-worker-a");

    let second_turn_started: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(4),
            params: TurnStartParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start worker b".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                cwd: None,
                approval_policy: None,
                approvals_reviewer: None,
                sandbox_policy: None,
                model: None,
                service_tier: None,
                effort: None,
                summary: None,
                personality: None,
                output_schema: None,
                collaboration_mode: None,
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("second turn/start should route to worker B");
    assert_eq!(second_turn_started.turn.id, "turn-thread-worker-b");

    let first_steer: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(5),
            params: TurnSteerParams {
                thread_id: first_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer worker a".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: first_turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("first turn/steer should route to worker A");
    assert_eq!(first_steer.turn_id, first_turn_started.turn.id);

    let second_steer: TurnSteerResponse = client
        .request_typed(ClientRequest::TurnSteer {
            request_id: RequestId::Integer(6),
            params: TurnSteerParams {
                thread_id: second_started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "steer worker b".to_string(),
                    text_elements: Vec::new(),
                }],
                responsesapi_client_metadata: None,
                expected_turn_id: second_turn_started.turn.id.clone(),
                ..Default::default()
            },
        })
        .await
        .expect("second turn/steer should route to worker B");
    assert_eq!(second_steer.turn_id, second_turn_started.turn.id);

    let first_interrupt: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(7),
            params: TurnInterruptParams {
                thread_id: first_started.thread.id.clone(),
                turn_id: first_turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("first turn/interrupt should route to worker A");
    assert_eq!(first_interrupt, TurnInterruptResponse {});

    let second_interrupt: TurnInterruptResponse = client
        .request_typed(ClientRequest::TurnInterrupt {
            request_id: RequestId::Integer(8),
            params: TurnInterruptParams {
                thread_id: second_started.thread.id.clone(),
                turn_id: second_turn_started.turn.id.clone(),
            },
        })
        .await
        .expect("second turn/interrupt should route to worker B");
    assert_eq!(second_interrupt, TurnInterruptResponse {});

    let mut coverage_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            TurnControlCoverage::default(),
        ),
        (
            second_started.thread.id.clone(),
            TurnControlCoverage::default(),
        ),
    ]);
    let expected_turn_by_thread = HashMap::from([
        (
            first_started.thread.id.clone(),
            ("turn-thread-worker-a", "worker a steer delta"),
        ),
        (
            second_started.thread.id.clone(),
            ("turn-thread-worker-b", "worker b steer delta"),
        ),
    ]);

    let turn_control_result = timeout(Duration::from_secs(5), async {
        while coverage_by_thread.values().any(|coverage| {
            *coverage
                != TurnControlCoverage {
                    saw_thread_active: true,
                    saw_turn_started: true,
                    saw_agent_delta: true,
                    saw_turn_completed: true,
                    saw_thread_idle: true,
                }
        }) {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    notification,
                )) => {
                    if let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id) {
                        match notification.status {
                            ThreadStatus::Active { ref active_flags }
                                if active_flags.is_empty() =>
                            {
                                coverage.saw_thread_active = true;
                            }
                            ThreadStatus::Idle => {
                                coverage.saw_thread_idle = true;
                            }
                            _ => {}
                        }
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnStarted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                    {
                        coverage.saw_turn_started = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                    notification,
                )) => {
                    if let Some((expected_turn_id, expected_delta)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn_id == *expected_turn_id
                        && notification.delta == *expected_delta
                    {
                        coverage.saw_agent_delta = true;
                    }
                }
                AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                    notification,
                )) => {
                    if let Some((expected_turn_id, _)) =
                        expected_turn_by_thread.get(&notification.thread_id)
                        && let Some(coverage) = coverage_by_thread.get_mut(&notification.thread_id)
                        && notification.turn.id == *expected_turn_id
                        && notification.turn.status == TurnStatus::Completed
                    {
                        coverage.saw_turn_completed = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        turn_control_result.is_ok(),
        true,
        "turn control notifications should fan in from both workers: {coverage_by_thread:?}"
    );

    assert_eq!(
        coverage_by_thread.get(&first_started.thread.id),
        Some(&TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        })
    );
    assert_eq!(
        coverage_by_thread.get(&second_started.thread.id),
        Some(&TurnControlCoverage {
            saw_thread_active: true,
            saw_turn_started: true,
            saw_agent_delta: true,
            saw_turn_completed: true,
            saw_thread_idle: true,
        })
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
