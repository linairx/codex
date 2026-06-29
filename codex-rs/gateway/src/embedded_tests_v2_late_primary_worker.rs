use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_primary_worker_onboarding_and_feedback_flows_over_v2() {
    let worker_a = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-a",
            requires_openai_auth: false,
            rate_limits: vec![("codex", "Codex", 20)],
            models: vec![("shared-model", "Shared Model", true)],
            apps: vec![("shared-app", "Shared App")],
            mcp_status_names: vec!["shared-mcp"],
            shared_cwd: "/tmp/shared-repo",
            unique_cwd: "/tmp/worker-a-only",
        },
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_bootstrap_setup_server(
        MultiConnectionBootstrapSetupConfig {
            worker_label: "worker-b",
            requires_openai_auth: true,
            rate_limits: vec![("worker-b", "Worker B", 35)],
            models: vec![("worker-b-model", "Worker B Model", true)],
            apps: vec![("worker-b-app", "Worker B App")],
            mcp_status_names: vec!["worker-b-mcp"],
            shared_cwd: "/tmp/shared-repo",
            unique_cwd: "/tmp/worker-b-only",
        },
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
        channel_capacity: 8,
    })
    .await
    .expect("remote client should connect to multi-worker gateway");

    let canceled_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(1),
            params: LoginAccountParams::ChatgptDeviceCode,
        }),
    )
    .await
    .expect("account/login/start should finish in time")
    .expect("account/login/start should succeed through multi-worker gateway");
    let canceled_login_id = match canceled_login {
        LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            verification_url,
            user_code,
        } => {
            assert_eq!(login_id, "worker-a-login-1");
            assert_eq!(verification_url, "https://example.com/device");
            assert_eq!(user_code, "worker-a-CODE");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let cancel_login: CancelLoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(2),
            params: CancelLoginAccountParams {
                login_id: canceled_login_id,
            },
        }),
    )
    .await
    .expect("account/login/cancel should finish in time")
    .expect("account/login/cancel should succeed through multi-worker gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::Canceled);

    let completed_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(3),
            params: LoginAccountParams::Chatgpt {
                codex_streamlined_login: false,
                callback_port: None,
            },
        }),
    )
    .await
    .expect("second account/login/start should finish in time")
    .expect("second account/login/start should succeed through multi-worker gateway");
    let completed_login_id = match completed_login {
        LoginAccountResponse::Chatgpt { login_id, auth_url } => {
            assert_eq!(login_id, "worker-a-login-2");
            assert_eq!(auth_url, "https://example.com/login");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let login_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::AccountLoginCompleted(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("account/login/completed notification should arrive");
    assert_eq!(login_completed.login_id, Some(completed_login_id));
    assert_eq!(login_completed.success, true);
    assert_eq!(login_completed.error, None);

    let feedback: FeedbackUploadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(4),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway multi-worker parity regression".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        }),
    )
    .await
    .expect("feedback/upload should finish in time")
    .expect("feedback/upload should succeed through multi-worker gateway");
    assert_eq!(feedback.thread_id, "feedback-thread-worker-a");

    let add_credits: SendAddCreditsNudgeEmailResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(5),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        }),
    )
    .await
    .expect("account/sendAddCreditsNudgeEmail should finish in time")
    .expect("account/sendAddCreditsNudgeEmail should succeed through multi-worker gateway");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(6),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf worker-a-command".to_string(),
                ],
                process_id: Some("proc-worker-a".to_string()),
                tty: true,
                stream_stdin: true,
                stream_stdout_stderr: true,
                output_bytes_cap: None,
                disable_output_cap: false,
                disable_timeout: false,
                timeout_ms: None,
                cwd: None,
                env: None,
                size: Some(CommandExecTerminalSize { rows: 24, cols: 80 }),
                sandbox_policy: None,
                permission_profile: None,
            },
        }),
    )
    .await
    .expect("command/exec should finish in time")
    .expect("command/exec should succeed through multi-worker gateway");
    assert_eq!(
        command_exec,
        CommandExecResponse {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    );

    let command_output = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta notification should arrive");
    assert_eq!(command_output.process_id, "proc-worker-a");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "d29ya2VyLWEtY29tbWFuZA==");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(7),
            params: CommandExecWriteParams {
                process_id: "proc-worker-a".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        }),
    )
    .await
    .expect("command/exec/write should finish in time")
    .expect("command/exec/write should succeed through multi-worker gateway");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(8),
            params: CommandExecResizeParams {
                process_id: "proc-worker-a".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        }),
    )
    .await
    .expect("command/exec/resize should finish in time")
    .expect("command/exec/resize should succeed through multi-worker gateway");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(9),
            params: CommandExecTerminateParams {
                process_id: "proc-worker-a".to_string(),
            },
        }),
    )
    .await
    .expect("command/exec/terminate should finish in time")
    .expect("command/exec/terminate should succeed through multi-worker gateway");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

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
