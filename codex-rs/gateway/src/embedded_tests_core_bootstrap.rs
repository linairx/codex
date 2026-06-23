use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn embedded_gateway_environment_manager_preserves_remote_exec_server_url() {
    let environment_manager = gateway_environment_manager(
        &GatewayConfig {
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            ..GatewayConfig::default()
        },
        &Arg0DispatchPaths {
            codex_self_exe: Some(std::env::current_exe().expect("current exe")),
            ..Arg0DispatchPaths::default()
        },
    )
    .await
    .expect("environment manager");

    let environment = environment_manager
        .default_environment()
        .expect("default environment");
    assert_eq!(environment.exec_server_url(), Some("ws://127.0.0.1:9753"));
}

#[tokio::test]
async fn embedded_gateway_environment_manager_builds_local_environment() {
    let environment_manager = gateway_environment_manager(
        &GatewayConfig::default(),
        &Arg0DispatchPaths {
            codex_self_exe: Some(std::env::current_exe().expect("current exe")),
            ..Arg0DispatchPaths::default()
        },
    )
    .await
    .expect("environment manager");

    assert_eq!(environment_manager.default_environment_id(), Some("local"));
}

#[tokio::test]
async fn embedded_gateway_rejects_unreachable_exec_server_configuration() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");

    let result = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            exec_server_url: Some("ws://127.0.0.1:1".to_string()),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await;

    let err = match result {
        Ok(_server) => panic!("gateway startup should reject unreachable exec-server"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::Other);
    assert_eq!(
        err.to_string()
            .contains("failed to initialize remote exec-server environment"),
        true
    );
}

#[test]
fn embedded_gateway_execution_mode_tracks_exec_server_configuration() {
    assert_eq!(
        gateway_execution_mode(&GatewayConfig::default()),
        GatewayExecutionMode::InProcess
    );
    assert_eq!(
        gateway_execution_mode(&GatewayConfig {
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            ..GatewayConfig::default()
        }),
        GatewayExecutionMode::ExecServer
    );
}

#[tokio::test]
async fn remote_gateway_runtime_rejects_exec_server_url_configuration() {
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");

    let result = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            exec_server_url: Some("ws://127.0.0.1:9753".to_string()),
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: "ws://127.0.0.1:8081".to_string(),
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
    .await;

    let err = match result {
        Ok(_server) => panic!("remote runtime should reject gateway exec server config"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "remote gateway runtime does not support `--exec-server-url`; configure execution on the remote app-server workers instead"
    );
}

#[tokio::test]
async fn remote_gateway_runtime_rejects_blank_worker_websocket_url() {
    let config = Config::load_default_with_cli_overrides(Vec::new())
        .await
        .expect("config");

    let result = start_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            runtime_mode: GatewayRuntimeMode::Remote,
            remote_runtime: Some(GatewayRemoteRuntimeConfig {
                selection_policy: GatewayRemoteSelectionPolicy::RoundRobin,
                workers: vec![GatewayRemoteWorkerConfig {
                    websocket_url: "   ".to_string(),
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
    .await;

    let err = match result {
        Ok(_server) => panic!("remote runtime should reject blank worker websocket URL"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        err.to_string(),
        "remote worker 0 websocket URL must not be blank"
    );
}

#[tokio::test]
async fn gateway_rejects_zero_v2_transport_hardening_values() {
    let bind_address = "127.0.0.1:0".parse().expect("bind address");
    let cases = vec![
        (
            GatewayConfig {
                bind_address,
                v2_initialize_timeout: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero initialize timeout",
            "gateway v2 initialize timeout must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_client_send_timeout: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero client send timeout",
            "gateway v2 client send timeout must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_reconnect_retry_backoff: Duration::ZERO,
                ..GatewayConfig::default()
            },
            "gateway should reject zero reconnect retry backoff",
            "gateway v2 reconnect retry backoff must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_max_pending_server_requests: 0,
                ..GatewayConfig::default()
            },
            "gateway should reject zero pending server request limit",
            "gateway v2 max pending server requests must be greater than zero",
        ),
        (
            GatewayConfig {
                bind_address,
                v2_max_pending_client_requests: 0,
                ..GatewayConfig::default()
            },
            "gateway should reject zero pending client request limit",
            "gateway v2 max pending client requests must be greater than zero",
        ),
    ];

    for (gateway_config, panic_message, expected_message) in cases {
        let config = Config::load_default_with_cli_overrides(Vec::new())
            .await
            .expect("config");

        let result = start_gateway_server(
            gateway_config,
            Arg0DispatchPaths::default(),
            config,
            Vec::new(),
            LoaderOverrides::default(),
        )
        .await;

        let err = match result {
            Ok(_server) => panic!("{panic_message}"),
            Err(err) => err,
        };

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), expected_message);
    }
}

#[tokio::test]
async fn embedded_server_serves_thread_creation_requests() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("http response");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: ThreadResponse = response.json().await.expect("thread response");
    assert_eq!(body.thread.ephemeral, true);
    assert_eq!(body.thread.id.is_empty(), false);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_streams_thread_started_events_over_sse() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let mut events_response = client
        .get(format!("http://{}/v1/events", server.local_addr()))
        .send()
        .await
        .expect("event stream response");
    assert_eq!(events_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        events_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread create response");
    let thread: ThreadResponse = create_response.json().await.expect("thread response");

    let event_body = timeout(std::time::Duration::from_secs(5), async {
        let mut body = String::new();
        loop {
            let chunk = events_response
                .chunk()
                .await
                .expect("event stream chunk")
                .expect("event stream not closed");
            body.push_str(std::str::from_utf8(&chunk).expect("utf8"));
            if body.contains("\n\n") {
                break body;
            }
        }
    })
    .await
    .expect("timed out waiting for SSE event");

    assert_eq!(event_body.contains("event: thread/started"), true);
    assert_eq!(event_body.contains(&thread.thread.id), true);

    drop(events_response);
    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_scopes_threads_by_tenant_and_project_headers() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let create_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("thread create response");
    assert_eq!(create_response.status(), reqwest::StatusCode::OK);
    let thread: ThreadResponse = create_response.json().await.expect("thread response");

    let same_scope_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            thread.thread.id
        ))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-a")
        .send()
        .await
        .expect("same-scope read response");
    assert_eq!(same_scope_response.status(), reqwest::StatusCode::OK);

    let other_project_response = client
        .get(format!(
            "http://{}/v1/threads/{}",
            server.local_addr(),
            thread.thread.id
        ))
        .header("x-codex-tenant-id", "tenant-a")
        .header("x-codex-project-id", "project-b")
        .send()
        .await
        .expect("other-project read response");
    assert_eq!(
        other_project_response.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    let other_tenant_list = client
        .get(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-b")
        .header("x-codex-project-id", "project-a")
        .send()
        .await
        .expect("other-tenant list response");
    assert_eq!(other_tenant_list.status(), reqwest::StatusCode::OK);
    let body: ListThreadsResponse = other_tenant_list.json().await.expect("list response");
    assert_eq!(body.data.is_empty(), true);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_applies_per_scope_request_rate_limits() {
    let codex_home = tempdir().expect("tempdir");
    let config = Config::load_default_with_cli_overrides_for_codex_home(
        codex_home.path().to_path_buf(),
        Vec::new(),
    )
    .await
    .expect("config");
    let server = start_embedded_gateway_server(
        GatewayConfig {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            request_rate_limit_per_minute: Some(1),
            ..GatewayConfig::default()
        },
        Arg0DispatchPaths::default(),
        config,
        Vec::new(),
        LoaderOverrides::default(),
    )
    .await
    .expect("server");

    let client = reqwest::Client::new();
    let first_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("first response");
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);

    let limited_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-a")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("limited response");
    assert_eq!(
        limited_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        limited_response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .is_some(),
        true
    );

    let other_scope_response = client
        .post(format!("http://{}/v1/threads", server.local_addr()))
        .header("x-codex-tenant-id", "tenant-b")
        .json(&CreateThreadRequest {
            cwd: Some(codex_home.path().display().to_string()),
            model: None,
            ephemeral: Some(true),
        })
        .send()
        .await
        .expect("other scope response");
    assert_eq!(other_scope_response.status(), reqwest::StatusCode::OK);

    server.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn embedded_server_supports_drop_in_v2_client_bootstrap_and_thread_workflow() {
    let codex_home = tempdir().expect("tempdir");
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

    let account: GetAccountResponse = client
        .request_typed(ClientRequest::GetAccount {
            request_id: RequestId::Integer(1),
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read should succeed through embedded gateway");
    assert_eq!(account.account, None);

    let models: ModelListResponse = client
        .request_typed(ClientRequest::ModelList {
            request_id: RequestId::Integer(2),
            params: ModelListParams {
                cursor: None,
                limit: None,
                include_hidden: Some(true),
            },
        })
        .await
        .expect("model/list should succeed through embedded gateway");
    assert_eq!(models.data.is_empty(), false);

    let started: AppServerThreadStartResponse = client
        .request_typed(ClientRequest::ThreadStart {
            request_id: RequestId::Integer(3),
            params: ThreadStartParams {
                model: None,
                model_provider: None,
                service_tier: None,
                cwd: Some(codex_home.path().display().to_string()),
                approval_policy: None,
                approvals_reviewer: None,
                sandbox: None,
                config: None,
                service_name: None,
                base_instructions: None,
                developer_instructions: None,
                personality: None,
                ephemeral: Some(false),
                session_start_source: None,
                dynamic_tools: None,
                mock_experimental_field: None,
                experimental_raw_events: false,
                ..Default::default()
            },
        })
        .await
        .expect("thread/start should succeed through embedded gateway");
    assert_eq!(started.thread.ephemeral, false);
    assert_eq!(started.thread.id.is_empty(), false);

    let listed: AppServerThreadListResponse = client
        .request_typed(ClientRequest::ThreadList {
            request_id: RequestId::Integer(4),
            params: ThreadListParams {
                parent_thread_id: None,
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
        })
        .await
        .expect("thread/list should succeed through embedded gateway");
    assert_eq!(listed.next_cursor, None);

    let loaded: ThreadLoadedListResponse = client
        .request_typed(ClientRequest::ThreadLoadedList {
            request_id: RequestId::Integer(5),
            params: ThreadLoadedListParams {
                cursor: None,
                limit: Some(10),
            },
        })
        .await
        .expect("thread/loaded/list should succeed through embedded gateway");
    assert_eq!(loaded.data.contains(&started.thread.id), true);

    let read: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(6),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read should succeed through embedded gateway");
    let mut expected_thread = started.thread.clone();
    expected_thread.created_at = read.thread.created_at;
    expected_thread.updated_at = read.thread.updated_at;
    expected_thread.recency_at = read.thread.recency_at;
    assert_eq!(read.thread, expected_thread);

    let renamed_thread_name = "Gateway Embedded Thread".to_string();
    let rename_response: ThreadSetNameResponse = client
        .request_typed(ClientRequest::ThreadSetName {
            request_id: RequestId::Integer(7),
            params: ThreadSetNameParams {
                thread_id: started.thread.id.clone(),
                name: renamed_thread_name.clone(),
            },
        })
        .await
        .expect("thread/name/set should succeed through embedded gateway");
    assert_eq!(rename_response, ThreadSetNameResponse {});

    let rename_notification = timeout(Duration::from_secs(5), async {
        loop {
            let event = client
                .next_event()
                .await
                .expect("event stream should stay open");
            if let AppServerEvent::ServerNotification(ServerNotification::ThreadNameUpdated(
                notification,
            )) = event
                && notification.thread_id == started.thread.id
            {
                break notification;
            }
        }
    })
    .await
    .expect("thread/name/updated notification should arrive");
    assert_eq!(
        rename_notification.thread_name,
        Some(renamed_thread_name.clone())
    );

    let renamed: AppServerThreadReadResponse = client
        .request_typed(ClientRequest::ThreadRead {
            request_id: RequestId::Integer(8),
            params: ThreadReadParams {
                thread_id: started.thread.id.clone(),
                include_turns: false,
            },
        })
        .await
        .expect("thread/read after rename should succeed through embedded gateway");
    assert_eq!(renamed.thread.name, Some(renamed_thread_name.clone()));

    let memory_mode_response: ThreadMemoryModeSetResponse = client
        .request_typed(ClientRequest::ThreadMemoryModeSet {
            request_id: RequestId::Integer(9),
            params: ThreadMemoryModeSetParams {
                thread_id: started.thread.id.clone(),
                mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("thread/memoryMode/set should succeed through embedded gateway");
    assert_eq!(memory_mode_response, ThreadMemoryModeSetResponse {});

    assert_remote_client_shutdown(client.shutdown().await);
    server.shutdown().await.expect("shutdown");
}
