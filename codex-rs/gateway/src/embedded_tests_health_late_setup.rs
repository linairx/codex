use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_v2_session_readds_recovered_primary_worker_for_setup_flows() {
    let worker_a = start_reconnecting_v2_multi_connection_bootstrap_setup_server(
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
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
    .expect("worker should reconnect before same-session setup-flow test starts");

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

    sleep(Duration::from_millis(200)).await;

    let config_requirements: ConfigRequirementsReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ConfigRequirementsRead {
            request_id: RequestId::Integer(1),
            params: None,
        }),
    )
    .await
    .expect("configRequirements/read should finish in time")
    .expect("configRequirements/read should succeed through recovered primary worker");
    assert_eq!(config_requirements.requirements, None);

    let canceled_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(2),
            params: LoginAccountParams::ChatgptDeviceCode,
        }),
    )
    .await
    .expect("account/login/start should finish in time")
    .expect("device-code login should succeed through recovered primary worker");
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
            request_id: RequestId::Integer(3),
            params: CancelLoginAccountParams {
                login_id: canceled_login_id,
            },
        }),
    )
    .await
    .expect("account/login/cancel should finish in time")
    .expect("account/login/cancel should succeed through recovered primary worker");
    assert_eq!(cancel_login.status, CancelLoginAccountStatus::Canceled);

    let completed_login: LoginAccountResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::LoginAccount {
            request_id: RequestId::Integer(4),
            params: LoginAccountParams::Chatgpt {
                codex_streamlined_login: false,
                callback_port: None,
            },
        }),
    )
    .await
    .expect("second account/login/start should finish in time")
    .expect("managed login should succeed through recovered primary worker");
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
            request_id: RequestId::Integer(5),
            params: FeedbackUploadParams {
                classification: "bug".to_string(),
                reason: Some("gateway same-session primary-worker recovery".to_string()),
                thread_id: None,
                include_logs: false,
                extra_log_files: None,
                tags: None,
            },
        }),
    )
    .await
    .expect("feedback/upload should finish in time")
    .expect("feedback/upload should succeed through recovered primary worker");
    assert_eq!(feedback.thread_id, "feedback-thread-worker-a");

    let add_credits: SendAddCreditsNudgeEmailResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::SendAddCreditsNudgeEmail {
            request_id: RequestId::Integer(10),
            params: SendAddCreditsNudgeEmailParams {
                credit_type: AddCreditsNudgeCreditType::Credits,
            },
        }),
    )
    .await
    .expect("account/sendAddCreditsNudgeEmail should finish in time")
    .expect("account/sendAddCreditsNudgeEmail should succeed through recovered primary worker");
    assert_eq!(add_credits.status, AddCreditsNudgeEmailStatus::Sent);

    let windows_setup: WindowsSandboxSetupStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(11),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/worker-a-primary")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        }),
    )
    .await
    .expect("windowsSandbox/setupStart should finish in time")
    .expect("windowsSandbox/setupStart should succeed through recovered primary worker");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted notification should arrive");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);

    let read_file: FsReadFileResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(12),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readFile should finish in time")
    .expect("fs/readFile should succeed through recovered primary worker");
    assert_eq!(read_file.data_base64, "d29ya2VyLWEtZmlsZQ==");

    let create_directory: FsCreateDirectoryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(17),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        }),
    )
    .await
    .expect("fs/createDirectory should finish in time")
    .expect("fs/createDirectory should succeed through recovered primary worker");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(18),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVjb3ZlcmVkLXByaW1hcnktZmlsZQ==".to_string(),
            },
        }),
    )
    .await
    .expect("fs/writeFile should finish in time")
    .expect("fs/writeFile should succeed through recovered primary worker");
    assert_eq!(write_file, FsWriteFileResponse {});

    let metadata: FsGetMetadataResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(19),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/getMetadata should finish in time")
    .expect("fs/getMetadata should succeed through recovered primary worker");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(20),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readDirectory should finish in time")
    .expect("fs/readDirectory should succeed through recovered primary worker");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "worker-a-gateway.txt");

    let copy: FsCopyResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(21),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/worker-a-primary/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/worker-a-primary/nested/copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        }),
    )
    .await
    .expect("fs/copy should finish in time")
    .expect("fs/copy should succeed through recovered primary worker");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(22),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/worker-a-primary/nested/copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(false),
                force: Some(true),
            },
        }),
    )
    .await
    .expect("fs/remove should finish in time")
    .expect("fs/remove should succeed through recovered primary worker");
    assert_eq!(remove, FsRemoveResponse {});

    let fuzzy_search: FuzzyFileSearchResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(13),
            params: FuzzyFileSearchParams {
                query: "gate".to_string(),
                roots: vec!["/tmp/worker-a-primary".to_string()],
                cancellation_token: Some("same-session-recovered-primary-fuzzy".to_string()),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch should finish in time")
    .expect("fuzzyFileSearch should succeed through recovered workers");
    assert_eq!(
        fuzzy_search.files,
        vec![
            FuzzyFileSearchResult {
                root: "/tmp/worker-b-search".to_string(),
                path: "src/gateway.rs".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.rs".to_string(),
                score: 60,
                indices: Some(vec![4, 5, 6, 7]),
            },
            FuzzyFileSearchResult {
                root: "/tmp/worker-a-primary".to_string(),
                path: "docs/gateway.md".to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: "gateway.md".to_string(),
                score: 42,
                indices: Some(vec![5, 6, 7, 8]),
            },
        ]
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(14),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "same-session-recovered-primary-fuzzy-session".to_string(),
                roots: vec!["/tmp/worker-a-primary".to_string()],
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStart should finish in time")
    .expect("fuzzyFileSearch/sessionStart should succeed through recovered primary worker");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(15),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "same-session-recovered-primary-fuzzy-session".to_string(),
                query: "gate".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionUpdate should finish in time")
    .expect("fuzzyFileSearch/sessionUpdate should succeed through recovered primary worker");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated notification should arrive");
    assert_eq!(
        fuzzy_session_updated.session_id,
        "same-session-recovered-primary-fuzzy-session"
    );
    assert_eq!(fuzzy_session_updated.query, "gate");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(fuzzy_session_updated.files[0].root, "/tmp/worker-a-primary");
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/gateway.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(16),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "same-session-recovered-primary-fuzzy-session".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStop should finish in time")
    .expect("fuzzyFileSearch/sessionStop should succeed through recovered primary worker");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted notification should arrive");
    assert_eq!(
        fuzzy_session_completed.session_id,
        "same-session-recovered-primary-fuzzy-session"
    );

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(6),
            params: CommandExecParams {
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "printf ready".to_string(),
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
    .expect("command/exec should succeed through recovered primary worker");
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
    assert_eq!(command_output.delta_base64, "cmVhZHk=");
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
    .expect("command/exec/write should succeed through recovered primary worker");
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
    .expect("command/exec/resize should succeed through recovered primary worker");
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
    .expect("command/exec/terminate should succeed through recovered primary worker");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
