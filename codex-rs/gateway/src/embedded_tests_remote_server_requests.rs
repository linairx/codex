use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_command_and_file_approval_roundtrips_over_v2() {
    let codex_linux_sandbox_exe = core_test_support::codex_linux_sandbox_exe_or_skip!();
    let codex_home = tempdir().expect("tempdir");
    let workspace = codex_home.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace should be created");
    let patch = r#"*** Begin Patch
*** Add File: README.md
+new line
*** End Patch
"#;
    let model_server = start_mock_responses_server_sequence(vec![
        create_exec_command_sse_response("exec-call")
            .expect("exec command response should serialize"),
        mock_responses_sse_body("command complete"),
        create_apply_patch_sse_response(patch, "patch-call")
            .expect("apply patch response should serialize"),
        mock_responses_sse_body("patch complete"),
    ])
    .await;
    std::fs::write(
        codex_home.path().join("config.toml"),
        format!(
            r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            model_server.uri
        ),
    )
    .expect("config.toml should be written");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            enable_codex_api_key_env: false,
            session_source: SessionSource::Cli,
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths {
            codex_linux_sandbox_exe,
            ..Arg0DispatchPaths::default()
        },
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
    .expect("remote client should connect to embedded gateway");

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(workspace.display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");

    let command_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(2),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "run a command".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("command turn/start should succeed through embedded gateway");
    let command_turn_id = command_turn.turn.id.clone();

    let (command_request_id, command_params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::CommandExecutionRequestApproval {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("command approval request should arrive");
    assert_eq!(command_params.thread_id, started.thread.id);
    assert_eq!(command_params.turn_id, command_turn_id);
    assert_eq!(command_params.item_id, "exec-call");
    let resolved_command_request_id = command_request_id.clone();

    client
        .resolve_server_request(
            command_request_id,
            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Accept,
            })
            .expect("command approval response should serialize"),
        )
        .await
        .expect("command approval request should resolve");

    let mut saw_command_resolved = false;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(notification) = event {
                match notification {
                    ServerNotification::ServerRequestResolved(
                        ServerRequestResolvedNotification {
                            thread_id,
                            request_id,
                        },
                    ) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(request_id, resolved_command_request_id);
                        saw_command_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, command_turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_command_resolved, true,
                            "serverRequest/resolved should arrive first"
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("command turn should resolve and complete");

    let file_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(3),
            params: TurnStartParams {
                thread_id: started.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "apply patch".to_string(),
                    text_elements: Vec::new(),
                }],
                cwd: Some(workspace.clone()),
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("file turn/start should succeed through embedded gateway");
    let file_turn_id = file_turn.turn.id.clone();

    let (file_request_id, file_params) = timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerRequest(ServerRequest::FileChangeRequestApproval {
                request_id,
                params,
            }) = event
            {
                break (request_id, params);
            }
        }
    })
    .await
    .expect("file approval request should arrive");
    assert_eq!(file_params.thread_id, started.thread.id);
    assert_eq!(file_params.turn_id, file_turn_id);
    assert_eq!(file_params.item_id, "patch-call");
    let resolved_file_request_id = file_request_id.clone();

    client
        .resolve_server_request(
            file_request_id,
            serde_json::to_value(FileChangeRequestApprovalResponse {
                decision: FileChangeApprovalDecision::Accept,
            })
            .expect("file approval response should serialize"),
        )
        .await
        .expect("file approval request should resolve");

    let mut saw_file_resolved = false;
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(notification) = event {
                match notification {
                    ServerNotification::ServerRequestResolved(
                        ServerRequestResolvedNotification {
                            thread_id,
                            request_id,
                        },
                    ) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(request_id, resolved_file_request_id);
                        saw_file_resolved = true;
                    }
                    ServerNotification::TurnCompleted(TurnCompletedNotification {
                        thread_id,
                        turn,
                    }) => {
                        assert_eq!(thread_id, started.thread.id);
                        assert_eq!(turn.id, file_turn_id);
                        assert_eq!(turn.status, TurnStatus::Completed);
                        assert_eq!(
                            saw_file_resolved, true,
                            "serverRequest/resolved should arrive first"
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("file turn should resolve and complete");

    assert_eq!(
        std::fs::read_to_string(workspace.join("README.md"))
            .expect("README should be written after accepting patch"),
        "new line\n"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}
