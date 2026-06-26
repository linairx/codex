use super::*;
use pretty_assertions::assert_eq;

pub(super) async fn assert_bootstrap_refresh_tail_requests(v2_client: &mut RemoteAppServerClient) {
    let create_directory: FsCreateDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCreateDirectory {
            request_id: RequestId::Integer(29),
            params: FsCreateDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/createDirectory path should be absolute"),
                recursive: Some(true),
            },
        }),
    )
    .await
    .expect("fs/createDirectory should finish in time after worker reconnect")
    .expect("fs/createDirectory should succeed after worker reconnect");
    assert_eq!(create_directory, FsCreateDirectoryResponse {});

    let write_file: FsWriteFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWriteFile {
            request_id: RequestId::Integer(30),
            params: FsWriteFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/writeFile path should be absolute"),
                data_base64: "cmVjb25uZWN0ZWQtZmlsZQ==".to_string(),
            },
        }),
    )
    .await
    .expect("fs/writeFile should finish in time after worker reconnect")
    .expect("fs/writeFile should succeed after worker reconnect");
    assert_eq!(write_file, FsWriteFileResponse {});

    let read_file: FsReadFileResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadFile {
            request_id: RequestId::Integer(31),
            params: FsReadFileParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/readFile path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readFile should finish in time after worker reconnect")
    .expect("fs/readFile should succeed after worker reconnect");
    assert_eq!(read_file.data_base64, "cmVjb25uZWN0ZWQtZmlsZQ==");

    let metadata: FsGetMetadataResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsGetMetadata {
            request_id: RequestId::Integer(32),
            params: FsGetMetadataParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/getMetadata path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/getMetadata should finish in time after worker reconnect")
    .expect("fs/getMetadata should succeed after worker reconnect");
    assert_eq!(metadata.is_file, true);
    assert_eq!(metadata.is_directory, false);

    let directory: FsReadDirectoryResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsReadDirectory {
            request_id: RequestId::Integer(33),
            params: FsReadDirectoryParams {
                path: PathBuf::from("/tmp/reconnected-project/nested")
                    .try_into()
                    .expect("fs/readDirectory path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/readDirectory should finish in time after worker reconnect")
    .expect("fs/readDirectory should succeed after worker reconnect");
    assert_eq!(directory.entries.len(), 1);
    assert_eq!(directory.entries[0].file_name, "gateway.txt");

    let copy: FsCopyResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsCopy {
            request_id: RequestId::Integer(34),
            params: FsCopyParams {
                source_path: PathBuf::from("/tmp/reconnected-project/nested/gateway.txt")
                    .try_into()
                    .expect("fs/copy source path should be absolute"),
                destination_path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/copy destination path should be absolute"),
                recursive: false,
            },
        }),
    )
    .await
    .expect("fs/copy should finish in time after worker reconnect")
    .expect("fs/copy should succeed after worker reconnect");
    assert_eq!(copy, FsCopyResponse {});

    let remove: FsRemoveResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsRemove {
            request_id: RequestId::Integer(35),
            params: FsRemoveParams {
                path: PathBuf::from("/tmp/reconnected-project/nested/copy.txt")
                    .try_into()
                    .expect("fs/remove path should be absolute"),
                recursive: Some(false),
                force: Some(true),
            },
        }),
    )
    .await
    .expect("fs/remove should finish in time after worker reconnect")
    .expect("fs/remove should succeed after worker reconnect");
    assert_eq!(remove, FsRemoveResponse {});

    let watch: FsWatchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FsWatch {
            request_id: RequestId::Integer(36),
            params: FsWatchParams {
                watch_id: "reconnected-watch".to_string(),
                path: PathBuf::from("/tmp/reconnected-project/config.toml")
                    .try_into()
                    .expect("fs/watch path should be absolute"),
            },
        }),
    )
    .await
    .expect("fs/watch should finish in time after worker reconnect")
    .expect("fs/watch should succeed after worker reconnect");
    assert_eq!(
        watch.path.as_ref().to_string_lossy(),
        "/tmp/reconnected-project/config.toml"
    );

    let fs_changed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fs/watch");
            if let AppServerEvent::ServerNotification(ServerNotification::FsChanged(notification)) =
                event
            {
                break notification;
            }
        }
    })
    .await
    .expect("fs/changed should arrive after recovered fs/watch");
    assert_eq!(fs_changed.watch_id, "reconnected-watch");
    assert_eq!(
        fs_changed
            .changed_paths
            .iter()
            .map(|path| path.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec!["/tmp/reconnected-project/config.toml".to_string()]
    );

    let fuzzy_search: FuzzyFileSearchResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearch {
            request_id: RequestId::Integer(37),
            params: FuzzyFileSearchParams {
                query: "reconnect".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
                cancellation_token: Some("reconnected-fuzzy-search".to_string()),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch should finish in time after worker reconnect")
    .expect("fuzzyFileSearch should succeed after worker reconnect");
    assert_eq!(fuzzy_search.files.len(), 1);
    assert_eq!(fuzzy_search.files[0].root, "/tmp/reconnected-project");
    assert_eq!(fuzzy_search.files[0].path, "docs/reconnected.md");
    assert_eq!(
        fuzzy_search.files[0].match_type,
        FuzzyFileSearchMatchType::File
    );

    let fuzzy_session_start: FuzzyFileSearchSessionStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStart {
            request_id: RequestId::Integer(38),
            params: FuzzyFileSearchSessionStartParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                roots: vec!["/tmp/reconnected-project".to_string()],
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStart should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStart should succeed after worker reconnect");
    assert_eq!(fuzzy_session_start, FuzzyFileSearchSessionStartResponse {});

    let fuzzy_session_update: FuzzyFileSearchSessionUpdateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: RequestId::Integer(39),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "reconnected-fuzzy-session".to_string(),
                query: "reconnect".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionUpdate should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionUpdate should succeed after worker reconnect");
    assert_eq!(
        fuzzy_session_update,
        FuzzyFileSearchSessionUpdateResponse {}
    );

    let fuzzy_session_updated = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy update");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionUpdated(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
                && notification.query == "reconnect"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionUpdated should arrive after worker reconnect");
    assert_eq!(fuzzy_session_updated.files.len(), 1);
    assert_eq!(
        fuzzy_session_updated.files[0].root,
        "/tmp/reconnected-project"
    );
    assert_eq!(fuzzy_session_updated.files[0].path, "docs/reconnected.md");

    let fuzzy_session_stop: FuzzyFileSearchSessionStopResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::FuzzyFileSearchSessionStop {
            request_id: RequestId::Integer(40),
            params: FuzzyFileSearchSessionStopParams {
                session_id: "reconnected-fuzzy-session".to_string(),
            },
        }),
    )
    .await
    .expect("fuzzyFileSearch/sessionStop should finish in time after worker reconnect")
    .expect("fuzzyFileSearch/sessionStop should succeed after worker reconnect");
    assert_eq!(fuzzy_session_stop, FuzzyFileSearchSessionStopResponse {});

    let fuzzy_session_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered fuzzy stop");
            if let AppServerEvent::ServerNotification(
                ServerNotification::FuzzyFileSearchSessionCompleted(notification),
            ) = event
                && notification.session_id == "reconnected-fuzzy-session"
            {
                break notification;
            }
        }
    })
    .await
    .expect("fuzzyFileSearch/sessionCompleted should arrive after worker reconnect");
    assert_eq!(
        fuzzy_session_completed.session_id,
        "reconnected-fuzzy-session"
    );

    let windows_setup: WindowsSandboxSetupStartResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::WindowsSandboxSetupStart {
            request_id: RequestId::Integer(41),
            params: WindowsSandboxSetupStartParams {
                mode: WindowsSandboxSetupMode::Unelevated,
                cwd: Some(
                    PathBuf::from("/tmp/reconnected-project")
                        .try_into()
                        .expect("windowsSandbox/setupStart cwd should be absolute"),
                ),
            },
        }),
    )
    .await
    .expect("windowsSandbox/setupStart should finish in time after worker reconnect")
    .expect("windowsSandbox/setupStart should succeed after worker reconnect");
    assert_eq!(windows_setup.started, true);

    let windows_setup_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered windows setup");
            if let AppServerEvent::ServerNotification(
                ServerNotification::WindowsSandboxSetupCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("windowsSandbox/setupCompleted should arrive after worker reconnect");
    assert_eq!(
        windows_setup_completed.mode,
        WindowsSandboxSetupMode::Unelevated
    );
    assert_eq!(windows_setup_completed.success, true);
    assert_eq!(windows_setup_completed.error, None);

    let command_exec: CommandExecResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::OneOffCommandExec {
            request_id: RequestId::Integer(42),
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
        }),
    )
    .await
    .expect("command/exec should finish in time after worker reconnect")
    .expect("command/exec should succeed after worker reconnect");
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
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open after recovered command/exec");
            if let AppServerEvent::ServerNotification(ServerNotification::CommandExecOutputDelta(
                notification,
            )) = event
                && notification.process_id == "proc-remote"
            {
                break notification;
            }
        }
    })
    .await
    .expect("command/exec/outputDelta should arrive after worker reconnect");
    assert_eq!(command_output.stream, CommandExecOutputStream::Stdout);
    assert_eq!(command_output.delta_base64, "cmVtb3RlLWNvbW1hbmQ=");
    assert_eq!(command_output.cap_reached, false);

    let command_write: CommandExecWriteResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecWrite {
            request_id: RequestId::Integer(43),
            params: CommandExecWriteParams {
                process_id: "proc-remote".to_string(),
                delta_base64: Some("AQID".to_string()),
                close_stdin: false,
            },
        }),
    )
    .await
    .expect("command/exec/write should finish in time after worker reconnect")
    .expect("command/exec/write should succeed after worker reconnect");
    assert_eq!(command_write, CommandExecWriteResponse {});

    let command_resize: CommandExecResizeResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecResize {
            request_id: RequestId::Integer(44),
            params: CommandExecResizeParams {
                process_id: "proc-remote".to_string(),
                size: CommandExecTerminalSize {
                    rows: 40,
                    cols: 120,
                },
            },
        }),
    )
    .await
    .expect("command/exec/resize should finish in time after worker reconnect")
    .expect("command/exec/resize should succeed after worker reconnect");
    assert_eq!(command_resize, CommandExecResizeResponse {});

    let command_terminate: CommandExecTerminateResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::CommandExecTerminate {
            request_id: RequestId::Integer(45),
            params: CommandExecTerminateParams {
                process_id: "proc-remote".to_string(),
            },
        }),
    )
    .await
    .expect("command/exec/terminate should finish in time after worker reconnect")
    .expect("command/exec/terminate should succeed after worker reconnect");
    assert_eq!(command_terminate, CommandExecTerminateResponse {});

    let mcp_oauth_login: McpServerOauthLoginResponse = timeout(
        Duration::from_secs(5),
        v2_client.request_typed(ClientRequest::McpServerOauthLogin {
            request_id: RequestId::Integer(46),
            params: McpServerOauthLoginParams {
                name: "reconnected-mcp".to_string(),
                scopes: Some(vec!["calendar.read".to_string()]),
                timeout_secs: Some(30),
                thread_id: None,
            },
        }),
    )
    .await
    .expect("mcpServer/oauth/login should finish in time after worker reconnect")
    .expect("mcpServer/oauth/login should succeed after worker reconnect");
    assert_eq!(
        mcp_oauth_login.authorization_url,
        "https://example.com/oauth/reconnected-mcp"
    );

    let mcp_oauth_completed = timeout(Duration::from_secs(5), async {
        loop {
            let event = v2_client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(
                ServerNotification::McpServerOauthLoginCompleted(notification),
            ) = event
            {
                break notification;
            }
        }
    })
    .await
    .expect("mcpServer/oauthLogin/completed should arrive after worker reconnect");
    assert_eq!(mcp_oauth_completed.name, "reconnected-mcp");
    assert_eq!(mcp_oauth_completed.success, true);
    assert_eq!(mcp_oauth_completed.error, None);
}
