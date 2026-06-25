use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn remote_multi_worker_routes_thread_mutations_to_owning_workers() {
    let worker_a =
        start_mock_remote_multi_connection_mutation_server("thread-worker-a", "/tmp/worker-a")
            .await;
    let worker_b =
        start_mock_remote_multi_connection_mutation_server("thread-worker-b", "/tmp/worker-b")
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

    let expected_started_notifications = HashSet::from([
        first_started.thread.id.clone(),
        second_started.thread.id.clone(),
    ]);
    let mut started_notifications = HashSet::new();
    timeout(Duration::from_secs(5), async {
        while started_notifications != expected_started_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadStarted(
                notification,
            )) = event
                && expected_started_notifications.contains(&notification.thread.id)
            {
                started_notifications.insert(notification.thread.id);
            }
        }
    })
    .await
    .expect("thread/started notifications should fan in from both workers");

    let first_name = "Worker A Thread".to_string();
    let second_name = "Worker B Thread".to_string();

    let first_rename: ThreadSetNameResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(3),
            params: ThreadSetNameParams {
                thread_id: first_started.thread.id.clone(),
                name: first_name.clone(),
            },
        }),
    )
    .await
    .expect("first thread/name/set should finish in time")
    .expect("first thread/name/set should route to worker A");
    assert_eq!(first_rename, ThreadSetNameResponse {});

    let second_rename: ThreadSetNameResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(4),
            params: ThreadSetNameParams {
                thread_id: second_started.thread.id.clone(),
                name: second_name.clone(),
            },
        }),
    )
    .await
    .expect("second thread/name/set should finish in time")
    .expect("second thread/name/set should route to worker B");
    assert_eq!(second_rename, ThreadSetNameResponse {});

    let expected_rename_notifications = HashMap::from([
        (first_started.thread.id.clone(), first_name.clone()),
        (second_started.thread.id.clone(), second_name.clone()),
    ]);
    let mut rename_notifications = HashMap::new();
    timeout(Duration::from_secs(5), async {
        while rename_notifications != expected_rename_notifications {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && let Some(thread_name) = notification.thread_name
                && expected_rename_notifications
                    .get(&notification.thread_id)
                    .is_some_and(|expected| expected == &thread_name)
            {
                rename_notifications.insert(notification.thread_id, thread_name);
            }
        }
    })
    .await
    .expect("thread/name/updated notifications should fan in from both workers");

    let first_memory_mode: ThreadMemoryModeSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(5),
            params: ThreadMemoryModeSetParams {
                thread_id: first_started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        }),
    )
    .await
    .expect("first thread/memoryMode/set should finish in time")
    .expect("first thread/memoryMode/set should route to worker A");
    assert_eq!(first_memory_mode, ThreadMemoryModeSetResponse {});

    let second_memory_mode: ThreadMemoryModeSetResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(6),
            params: ThreadMemoryModeSetParams {
                thread_id: second_started.thread.id.clone(),
                mode: ThreadMemoryMode::Disabled,
            },
        }),
    )
    .await
    .expect("second thread/memoryMode/set should finish in time")
    .expect("second thread/memoryMode/set should route to worker B");
    assert_eq!(second_memory_mode, ThreadMemoryModeSetResponse {});

    let first_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(7),
            params: ThreadReadParams {
                thread_id: first_started.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("first thread/read should finish in time")
    .expect("thread/read should route renamed thread back to worker A");
    assert_eq!(first_read.thread.id, first_started.thread.id);
    assert_eq!(first_read.thread.name, Some(first_name.clone()));
    assert_eq!(
        first_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-a"
    );

    let second_read: AppServerThreadReadResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: second_started.thread.id.clone(),
                include_turns: false,
            },
        }),
    )
    .await
    .expect("second thread/read should finish in time")
    .expect("thread/read should route renamed thread back to worker B");
    assert_eq!(second_read.thread.id, second_started.thread.id);
    assert_eq!(second_read.thread.name, Some(second_name.clone()));
    assert_eq!(
        second_read.thread.cwd.as_ref().to_string_lossy(),
        "/tmp/worker-b"
    );

    let listed: AppServerThreadListResponse = timeout(
        Duration::from_secs(5),
        client.request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(9),
            params: ThreadListParams {
                parent_thread_id: None,
                ancestor_thread_id: None,
                use_state_db_only: false,
                cursor: None,
                limit: Some(10),
                sort_key: None,
                sort_direction: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            },
        }),
    )
    .await
    .expect("thread/list should finish in time")
    .expect("thread/list should aggregate renamed threads");
    let names_by_thread = listed
        .data
        .into_iter()
        .map(|thread| (thread.id, thread.name))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        names_by_thread.get(&first_started.thread.id),
        Some(&Some(first_name))
    );
    assert_eq!(
        names_by_thread.get(&second_started.thread.id),
        Some(&Some(second_name))
    );

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
