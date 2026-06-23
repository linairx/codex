use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_single_worker_supports_thread_control_and_review_workflow_after_worker_reconnect() {
    let websocket_url = start_reconnecting_v2_single_worker_thread_control_server(
        "thread-single-worker-reconnected",
        "/tmp/single-worker-reconnected",
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

    let health_client = reqwest::Client::new();
    timeout(Duration::from_secs(5), async {
        loop {
            let healthz_response = health_client
                .get(format!("http://{}/healthz", server.local_addr()))
                .send()
                .await
                .expect("healthz response");
            let health: GatewayHealthResponse = healthz_response.json().await.expect("health body");
            if health.status == GatewayHealthStatus::Ok
                && health
                    .remote_workers
                    .as_ref()
                    .and_then(|workers| workers.first())
                    .is_some_and(|worker| worker.healthy && worker.last_error.is_some())
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker should reconnect before thread-control recovery test starts");

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
        channel_capacity: 64,
    })
    .await
    .expect("remote client should connect after worker reconnect");

    sleep(Duration::from_millis(200)).await;

    let started: AppServerThreadStartResponse = timeout(Duration::from_secs(5), async {
        let mut attempt = 1;
        loop {
            match client
                .request_typed(ClientRequest::ThreadStart {
                    request_id: RequestId::Integer(attempt),
                    params: ThreadStartParams {
                        model: None,
                        model_provider: None,
                        service_tier: None,
                        cwd: Some("/tmp/single-worker-reconnected".to_string()),
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
            {
                Ok(started) => break started,
                Err(TypedRequestError::Transport { source, .. })
                    if source.kind() == ErrorKind::BrokenPipe =>
                {
                    attempt += 1;
                    sleep(Duration::from_millis(100)).await;
                }
                Err(err) => panic!("thread/start should succeed after worker reconnect: {err}"),
            }
        }
    })
    .await
    .expect("thread/start retry window should finish in time");
    assert_eq!(started.thread.id, "thread-single-worker-reconnected");

    let unsubscribe: ThreadUnsubscribeResponse = client
        .request_typed(ClientRequest::ThreadUnsubscribe {
            request_id: RequestId::Integer(2),
            params: ThreadUnsubscribeParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unsubscribe should succeed after worker reconnect");
    assert_eq!(
        unsubscribe,
        ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        }
    );

    let archive: ThreadArchiveResponse = client
        .request_typed(ClientRequest::ThreadArchive {
            request_id: RequestId::Integer(3),
            params: ThreadArchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/archive should succeed after worker reconnect");
    assert_eq!(archive, ThreadArchiveResponse {});

    let unarchive: ThreadUnarchiveResponse = client
        .request_typed(ClientRequest::ThreadUnarchive {
            request_id: RequestId::Integer(4),
            params: ThreadUnarchiveParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/unarchive should succeed after worker reconnect");
    assert_eq!(unarchive.thread.id, started.thread.id);
    assert_eq!(unarchive.thread.preview, "/tmp/single-worker-reconnected");

    let metadata_update: ThreadMetadataUpdateResponse = client
        .request_typed(ClientRequest::ThreadMetadataUpdate {
            request_id: RequestId::Integer(5),
            params: ThreadMetadataUpdateParams {
                thread_id: started.thread.id.clone(),
                git_info: Some(ThreadMetadataGitInfoUpdateParams {
                    sha: Some(Some("reconnected-sha".to_string())),
                    branch: Some(Some("main".to_string())),
                    origin_url: Some(None),
                }),
            },
        })
        .await
        .expect("thread/metadata/update should succeed after worker reconnect");
    assert_eq!(metadata_update.thread.id, started.thread.id);

    let turns: ThreadTurnsListResponse = client
        .request_typed(ClientRequest::ThreadTurnsList {
            request_id: RequestId::Integer(6),
            params: ThreadTurnsListParams {
                thread_id: started.thread.id.clone(),
                cursor: None,
                limit: Some(10),
                sort_direction: None,
                items_view: None,
            },
        })
        .await
        .expect("thread/turns/list should succeed after worker reconnect");
    assert_eq!(turns.data.len(), 1);
    assert_eq!(turns.data[0].id, "turn-thread-single-worker-reconnected");
    assert_eq!(turns.next_cursor, None);
    assert_eq!(turns.backwards_cursor, None);

    let increment_elicitation: ThreadIncrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadIncrementElicitation {
            request_id: RequestId::Integer(7),
            params: ThreadIncrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/increment_elicitation should succeed after worker reconnect");
    assert_eq!(
        increment_elicitation,
        ThreadIncrementElicitationResponse {
            count: 1,
            paused: true,
        }
    );

    let decrement_elicitation: ThreadDecrementElicitationResponse = client
        .request_typed(ClientRequest::ThreadDecrementElicitation {
            request_id: RequestId::Integer(8),
            params: ThreadDecrementElicitationParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/decrement_elicitation should succeed after worker reconnect");
    assert_eq!(
        decrement_elicitation,
        ThreadDecrementElicitationResponse {
            count: 0,
            paused: false,
        }
    );

    let inject_items: ThreadInjectItemsResponse = client
        .request_typed(ClientRequest::ThreadInjectItems {
            request_id: RequestId::Integer(9),
            params: ThreadInjectItemsParams {
                thread_id: started.thread.id.clone(),
                items: vec![serde_json::json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "reconnected seed item"}],
                })],
            },
        })
        .await
        .expect("thread/inject_items should succeed after worker reconnect");
    assert_eq!(inject_items, ThreadInjectItemsResponse {});

    let compact: ThreadCompactStartResponse = client
        .request_typed(ClientRequest::ThreadCompactStart {
            request_id: RequestId::Integer(10),
            params: ThreadCompactStartParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/compact/start should succeed after worker reconnect");
    assert_eq!(compact, ThreadCompactStartResponse {});

    let shell_command: ThreadShellCommandResponse = client
        .request_typed(ClientRequest::ThreadShellCommand {
            request_id: RequestId::Integer(11),
            params: ThreadShellCommandParams {
                thread_id: started.thread.id.clone(),
                command: "printf single-worker-reconnected".to_string(),
            },
        })
        .await
        .expect("thread/shellCommand should succeed after worker reconnect");
    assert_eq!(shell_command, ThreadShellCommandResponse {});

    let clean_terminals: ThreadBackgroundTerminalsCleanResponse = client
        .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
            request_id: RequestId::Integer(12),
            params: ThreadBackgroundTerminalsCleanParams {
                thread_id: started.thread.id.clone(),
            },
        })
        .await
        .expect("thread/backgroundTerminals/clean should succeed after worker reconnect");
    assert_eq!(clean_terminals, ThreadBackgroundTerminalsCleanResponse {});

    let rollback: ThreadRollbackResponse = client
        .request_typed(ClientRequest::ThreadRollback {
            request_id: RequestId::Integer(13),
            params: ThreadRollbackParams {
                thread_id: started.thread.id.clone(),
                num_turns: 1,
            },
        })
        .await
        .expect("thread/rollback should succeed after worker reconnect");
    assert_eq!(rollback.thread.id, started.thread.id);
    assert_eq!(rollback.thread.preview, "/tmp/single-worker-reconnected");

    let review: ReviewStartResponse = client
        .request_typed(ClientRequest::ReviewStart {
            request_id: RequestId::Integer(14),
            params: ReviewStartParams {
                thread_id: started.thread.id.clone(),
                target: ReviewTarget::Custom {
                    instructions: "Review after reconnect".to_string(),
                },
                delivery: Some(ReviewDelivery::Detached),
            },
        })
        .await
        .expect("review/start should succeed after worker reconnect");
    assert_eq!(
        review.turn.id,
        "turn-review-thread-single-worker-reconnected"
    );
    assert_eq!(
        review.review_thread_id,
        "thread-single-worker-reconnected-review"
    );

    let review_thread: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(15),
            params: ThreadReadParams {
                thread_id: review.review_thread_id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed for review thread after worker reconnect");
    assert_eq!(review_thread.thread.id, review.review_thread_id);
    assert_eq!(
        review_thread.thread.preview,
        "/tmp/single-worker-reconnected"
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
