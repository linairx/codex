use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_supports_v2_thread_control_and_review_routing() {
    let worker_a = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-a",
        "/tmp/worker-a",
        "turn-worker-a",
        "delta from worker a",
    )
    .await;
    let worker_b = start_mock_remote_multi_connection_workflow_server(
        "thread-worker-b",
        "/tmp/worker-b",
        "turn-worker-b",
        "delta from worker b",
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

    let first_unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(3),
            params: ThreadUnsubscribeParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/unsubscribe should route to worker A");
    assert_eq!(
        first_unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let second_unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(4),
            params: ThreadUnsubscribeParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/unsubscribe should route to worker B");
    assert_eq!(
        second_unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let first_archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(5),
            params: ThreadArchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/archive should route to worker A");
    assert_eq!(first_archive, ThreadArchiveResponse {});

    let second_archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(6),
            params: ThreadArchiveParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/archive should route to worker B");
    assert_eq!(second_archive, ThreadArchiveResponse {});

    let first_unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(7),
            params: ThreadUnarchiveParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/unarchive should route to worker A");
    assert_eq!(first_unarchive.thread.id, first_started.thread.id);
    assert_eq!(first_unarchive.thread.preview, "/tmp/worker-a");

    let second_unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(8),
            params: ThreadUnarchiveParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/unarchive should route to worker B");
    assert_eq!(second_unarchive.thread.id, second_started.thread.id);
    assert_eq!(second_unarchive.thread.preview, "/tmp/worker-b");

    let expected_thread_lifecycle_notifications = HashSet::from([
        (first_started.thread.id.clone(), "closed"),
        (second_started.thread.id.clone(), "closed"),
        (first_started.thread.id.clone(), "archived"),
        (second_started.thread.id.clone(), "archived"),
        (first_started.thread.id.clone(), "unarchived"),
        (second_started.thread.id.clone(), "unarchived"),
    ]);
    let mut thread_lifecycle_notifications = HashSet::new();
    let lifecycle_result = timeout(Duration::from_secs(5), async {
        while thread_lifecycle_notifications != expected_thread_lifecycle_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            match event {
                AppServerEvent::ServerNotification(ServerNotification::ThreadClosed(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "closed"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadArchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "archived"));
                }
                AppServerEvent::ServerNotification(ServerNotification::ThreadUnarchived(
                    notification,
                )) if notification.thread_id == first_started.thread.id
                    || notification.thread_id == second_started.thread.id =>
                {
                    thread_lifecycle_notifications.insert((notification.thread_id, "unarchived"));
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        lifecycle_result.is_ok(),
        "thread lifecycle notifications should fan in from both workers: {thread_lifecycle_notifications:?}"
    );

    let first_metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(9),
            params: ThreadMetadataUpdateParams {
                thread_id: first_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-worker-a".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("first thread/metadata/update should route to worker A");
    assert_eq!(first_metadata_update.thread.id, first_started.thread.id);

    let second_metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(10),
            params: ThreadMetadataUpdateParams {
                thread_id: second_started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("sha-worker-b".to_string())),
                    branch: Some(Some("develop".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("second thread/metadata/update should route to worker B");
    assert_eq!(second_metadata_update.thread.id, second_started.thread.id);

    let first_turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(11),
            params: ThreadTurnsListParams {
                thread_id: first_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("first thread/turns/list should route to worker A");
    assert_eq!(first_turns.data.len(), 1);
    assert_eq!(first_turns.data[0].id, "turn-worker-a");
    assert_eq!(first_turns.next_cursor, None);
    assert_eq!(first_turns.backwards_cursor, None);

    let second_turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(12),
            params: ThreadTurnsListParams {
                thread_id: second_started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("second thread/turns/list should route to worker B");
    assert_eq!(second_turns.data.len(), 1);
    assert_eq!(second_turns.data[0].id, "turn-worker-b");
    assert_eq!(second_turns.next_cursor, None);
    assert_eq!(second_turns.backwards_cursor, None);

    let first_increment: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(13),
            params: ThreadIncrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/increment_elicitation should route to worker A");
    assert_eq!(
        first_increment,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let second_increment: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(14),
            params: ThreadIncrementElicitationParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/increment_elicitation should route to worker B");
    assert_eq!(
        second_increment,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let first_decrement: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(15),
            params: ThreadDecrementElicitationParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/decrement_elicitation should route to worker A");
    assert_eq!(
        first_decrement,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let second_decrement: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(16),
            params: ThreadDecrementElicitationParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/decrement_elicitation should route to worker B");
    assert_eq!(
        second_decrement,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let first_inject: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(17),
            params: ThreadInjectItemsParams {
                thread_id: first_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "worker-a injected"}],
                })],
            },
        })
        .await
        .expect("first thread/inject_items should route to worker A");
    assert_eq!(first_inject, ThreadInjectItemsResponse {});

    let second_inject: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(18),
            params: ThreadInjectItemsParams {
                thread_id: second_started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "worker-b injected"}],
                })],
            },
        })
        .await
        .expect("second thread/inject_items should route to worker B");
    assert_eq!(second_inject, ThreadInjectItemsResponse {});

    let first_compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(19),
            params: ThreadCompactStartParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/compact/start should route to worker A");
    assert_eq!(first_compact, ThreadCompactStartResponse {});

    let second_compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(20),
            params: ThreadCompactStartParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/compact/start should route to worker B");
    assert_eq!(second_compact, ThreadCompactStartResponse {});

    let first_shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(21),
            params: ThreadShellCommandParams {
                thread_id: first_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("first thread/shellCommand should route to worker A");
    assert_eq!(first_shell_command, ThreadShellCommandResponse {});

    let second_shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(22),
            params: ThreadShellCommandParams {
                thread_id: second_started.thread.id.clone(),
                command: "pwd".to_string(),
            },
        })
        .await
        .expect("second thread/shellCommand should route to worker B");
    assert_eq!(second_shell_command, ThreadShellCommandResponse {});

    let first_clean: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(23),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: first_started.thread.id.clone(),
            },
        })
        .await
        .expect("first thread/backgroundTerminals/clean should route to worker A");
    assert_eq!(first_clean, ThreadBackgroundTerminalsCleanResponse {});

    let second_clean: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(24),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: second_started.thread.id.clone(),
            },
        })
        .await
        .expect("second thread/backgroundTerminals/clean should route to worker B");
    assert_eq!(second_clean, ThreadBackgroundTerminalsCleanResponse {});

    let first_rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(25),
            params: ThreadRollbackParams {
                thread_id: first_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("first thread/rollback should route to worker A");
    assert_eq!(first_rollback.thread.id, first_started.thread.id);
    assert_eq!(first_rollback.thread.preview, "/tmp/worker-a");

    let second_rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(26),
            params: ThreadRollbackParams {
                thread_id: second_started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("second thread/rollback should route to worker B");
    assert_eq!(second_rollback.thread.id, second_started.thread.id);
    assert_eq!(second_rollback.thread.preview, "/tmp/worker-b");

    let first_review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(27),
            params: ReviewStartParams {
                thread_id: first_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review worker A".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("first review/start should route to worker A");
    assert_eq!(first_review.turn.id, "turn-review-thread-worker-a");
    assert_eq!(first_review.review_thread_id, "thread-worker-a-review");

    let second_review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(28),
            params: ReviewStartParams {
                thread_id: second_started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review worker B".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("second review/start should route to worker B");
    assert_eq!(second_review.turn.id, "turn-review-thread-worker-b");
    assert_eq!(second_review.review_thread_id, "thread-worker-b-review");

    let first_review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(29),
            params: ThreadReadParams {
                thread_id: first_review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("first review thread/read should route to worker A");
    assert_eq!(first_review_thread.thread.id, first_review.review_thread_id);
    assert_eq!(first_review_thread.thread.preview, "/tmp/worker-a/review");

    let second_review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(30),
            params: ThreadReadParams {
                thread_id: second_review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("second review thread/read should route to worker B");
    assert_eq!(
        second_review_thread.thread.id,
        second_review.review_thread_id
    );
    assert_eq!(second_review_thread.thread.preview, "/tmp/worker-b/review");

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
