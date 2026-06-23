use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_thread_control_workflow() {
    let model_server = start_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = tempdir().expect("tempdir");
    std::fs::create_dir_all(codex_home.path().join(".git")).expect(".git should be created");
    write_mock_responses_config_toml(codex_home.path(), &model_server.uri)
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
    .expect("remote client should connect to embedded gateway");

    let unsubscribe_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(1),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("unsubscribe thread/start should succeed through embedded gateway");

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: unsubscribe_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed through embedded gateway");
    assert!(
        matches!(
            unsubscribe.status,
            ThreadUnsubscribeStatus::Unsubscribed | ThreadUnsubscribeStatus::NotSubscribed
        ),
        "unexpected unsubscribe status: {:?}",
        unsubscribe.status
    );

    let shell_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("shell thread/start should succeed through embedded gateway");

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(4),
            params: ThreadShellCommandParams {
                thread_id: shell_thread.thread.id.clone(),
                command: "printf embedded-gateway".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed through embedded gateway");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(5),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: shell_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed through embedded gateway");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let archive_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(6),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("archive thread/start should succeed through embedded gateway");

    let archive_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(7),
            params: TurnStartParams {
                thread_id: archive_thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "produce one turn".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("archive turn/start should succeed through embedded gateway");
    let archive_turn_id = archive_turn.turn.id.clone();

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == archive_thread.thread.id
                && turn.id == archive_turn_id
                && turn.status == TurnStatus::Completed
            {
                break;
            }
        }
    })
    .await
    .expect("archive seed turn should complete");

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(8),
            params: ThreadArchiveParams {
                thread_id: archive_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed through embedded gateway");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(9),
            params: ThreadUnarchiveParams {
                thread_id: archive_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed through embedded gateway");
    assert_eq!(unarchive.thread.id, archive_thread.thread.id);

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: archive_thread.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("abc123".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: None,
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed through embedded gateway");
    assert_eq!(metadata_update.thread.id, archive_thread.thread.id);

    let rollback_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(11),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("rollback thread/start should succeed through embedded gateway");

    let rollback_turn: TurnStartResponse = client
        .request_typed(ClientRequest::TurnStart {
            request_id: RequestId::Integer(12),
            params: TurnStartParams {
                thread_id: rollback_thread.thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "produce one turn".to_string(),
                    text_elements: Vec::new(),
                }],
                model: Some("mock-model".to_string()),
                ..Default::default()
            },
        })
        .await
        .expect("rollback turn/start should succeed through embedded gateway");
    let rollback_turn_id = rollback_turn.turn.id.clone();

    timeout(Duration::from_secs(10), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::TurnCompleted(
                TurnCompletedNotification { thread_id, turn },
            )) = event
                && thread_id == rollback_thread.thread.id
                && turn.id == rollback_turn_id
                && turn.status == TurnStatus::Completed
            {
                break;
            }
        }
    })
    .await
    .expect("rollback seed turn should complete");

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(13),
            params: ThreadTurnsListParams {
                thread_id: rollback_thread.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed through embedded gateway");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, rollback_turn_id);
    assert_eq!(turns.next_cursor, None);

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(14),
            params: ThreadRollbackParams {
                thread_id: rollback_thread.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed through embedded gateway");
    assert_eq!(rollback.thread.id, rollback_thread.thread.id);
    assert_eq!(rollback.thread.turns, Vec::new());

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(15),
            params: ThreadIncrementElicitationParams {
                thread_id: rollback_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed through embedded gateway");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadDecrementElicitationParams {
                thread_id: rollback_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed through embedded gateway");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(17),
            params: ThreadInjectItemsParams {
                thread_id: rollback_thread.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed through embedded gateway");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let compact_thread: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(18),
            params: ThreadStartParams {
                model: Some("mock-model".to_string()),
                cwd: Some(codex_home.path().display().to_string()),
                ephemeral: Some(false),
                ..Default::default()
            },
        })
        .await
        .expect("compact thread/start should succeed through embedded gateway");

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: compact_thread.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed through embedded gateway");
    assert_eq!(compact, ThreadCompactStartResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
    model_server.shutdown().await;
}
