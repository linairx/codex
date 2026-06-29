use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_setup_account_and_command(client: &mut RemoteAppServerClient) {
    let canceled_login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(22),
            params: LoginAccountParams::ChatgptDeviceCode,
        })
        .await
        .expect("account/login/start should succeed through remote gateway");
    let canceled_login_id = match canceled_login {
        LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            verification_url,
            user_code,
        } => {
            assert_eq!(verification_url, "https://example.com/device");
            assert_eq!(user_code, "REMOTE-CODE");
            login_id
        }
        other => panic!("unexpected account/login/start response: {other:?}"),
    };

    let cancel_login: CancelLoginAccountResponse = client
        .request_typed(ClientRequest::CancelLoginAccount {
            request_id: RequestId::Integer(23),
            params: CancelLoginAccountParams {
                login_id: canceled_login_id,
            },
        })
        .await
        .expect("account/login/cancel should succeed through remote gateway");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::Canceled);

    let completed_login: LoginAccountResponse = client
        .request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(24),
            params: LoginAccountParams::Chatgpt {
                codex_streamlined_login: false,
                callback_port: None,
            },
        })
        .await
        .expect("second account/login/start should succeed through remote gateway");
    let completed_login_id = match completed_login {
        LoginAccountResponse::Chatgpt { login_id, auth_url } => {
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

    let add_credits: SendAddCreditsNudgeEmailResponse = client
        .request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(47),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        })
        .await
        .expect("account/sendAddCreditsNudgeEmail should succeed through remote gateway");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let feedback: FeedbackUploadResponse = client
        .request_typed(ClientRequest::FeedbackUpload {
            request_id: RequestId::Integer(25),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway parity regression".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        })
        .await
        .expect("feedback/upload should succeed through remote gateway");
    assert_eq!(feedback.thread_id, "feedback-thread-remote");

    let command_exec: CommandExecResponse = client
        .request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(26),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf remote-command".to_string(),
                ],
                process_id: Some("proc-remote".to_string()),
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
        })
        .await
        .expect("command/exec should succeed through remote gateway");
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
    .expect("command/exec/outputDelta notification should arrive through remote gateway");
    assert_eq!(command_output.process_id, "proc-remote");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "cmVtb3RlLWNvbW1hbmQ=");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = client
        .request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(26),
            params: CommandExecWriteParams {
                process_id: "proc-remote".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        })
        .await
        .expect("command/exec/write should succeed through remote gateway");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = client
        .request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(27),
            params: CommandExecResizeParams {
                process_id: "proc-remote".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        })
        .await
        .expect("command/exec/resize should succeed through remote gateway");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = client
        .request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(28),
            params: CommandExecTerminateParams {
                process_id: "proc-remote".to_string(),
            },
        })
        .await
        .expect("command/exec/terminate should succeed through remote gateway");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    let reset: MemoryResetResponse = client
        .request_typed(ClientRequest::MemoryReset {
            request_id: RequestId::Integer(27),
            params: None,
        })
        .await
        .expect("memory/reset should succeed through remote gateway");
    assert_eq!(reset, MemoryResetResponse {});

    let logout: LogoutAccountResponse = client
        .request_typed(ClientRequest::LogoutAccount {
            request_id: RequestId::Integer(28),
            params: None,
        })
        .await
        .expect("account/logout should succeed through remote gateway");
    assert_eq!(logout, LogoutAccountResponse {});
}
