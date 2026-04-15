use super::connection_handling_websocket::DEFAULT_READ_TIMEOUT;
use super::connection_handling_websocket::WsClient;
use super::connection_handling_websocket::connect_websocket;
use super::connection_handling_websocket::create_config_toml;
use super::connection_handling_websocket::read_notification_for_method;
use super::connection_handling_websocket::read_response_for_id;
use super::connection_handling_websocket::send_initialize_request;
use super::connection_handling_websocket::send_request;
use super::connection_handling_websocket::spawn_websocket_server;
use anyhow::Context;
use anyhow::Result;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_shell_command_sse_response;
use app_test_support::to_response;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn websocket_disconnect_unloads_non_resident_thread() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri(), "never")?;

    let (mut process, bind_addr) = spawn_websocket_server(codex_home.path()).await?;

    let result = async {
        let mut ws1 = connect_websocket(bind_addr).await?;
        let mut ws2 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws1, /*id*/ 1, "ws_disconnect_owner").await?;
        initialize_client(&mut ws2, /*id*/ 2, "ws_disconnect_observer").await?;

        let thread = start_thread(&mut ws1, /*id*/ 10, /*resident*/ false).await?;

        drop(ws1);

        let (closed, status_changed) =
            wait_for_thread_closed_and_not_loaded(&mut ws2, &thread.id).await?;
        assert_eq!(closed.thread_id, thread.id);
        assert_eq!(status_changed.thread_id, thread.id);
        assert_eq!(status_changed.status, ThreadStatus::NotLoaded);

        let loaded_list = loaded_list(&mut ws2, /*id*/ 11).await?;
        assert_eq!(loaded_list.data, Vec::<String>::new());
        assert_eq!(loaded_list.next_cursor, None);
        Ok(())
    }
    .await;

    process
        .kill()
        .await
        .context("failed to stop websocket app-server process")?;
    result
}

#[tokio::test]
async fn websocket_disconnect_keeps_resident_thread_loaded() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri(), "never")?;

    let (mut process, bind_addr) = spawn_websocket_server(codex_home.path()).await?;

    let result = async {
        let mut ws1 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws1, /*id*/ 1, "ws_resident_owner").await?;

        let thread = start_thread(&mut ws1, /*id*/ 10, /*resident*/ true).await?;
        drop(ws1);

        let mut ws2 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws2, /*id*/ 2, "ws_resident_reconnect").await?;

        let loaded_list = loaded_list(&mut ws2, /*id*/ 11).await?;
        assert_eq!(loaded_list.data, vec![thread.id.clone()]);
        assert_eq!(loaded_list.next_cursor, None);

        let loaded_read = loaded_read(&mut ws2, /*id*/ 12).await?;
        let loaded_thread = loaded_read
            .data
            .into_iter()
            .find(|loaded_thread| loaded_thread.id == thread.id)
            .expect("thread/loaded/read should include resident thread after disconnect");
        assert!(loaded_thread.resident);
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(loaded_thread.status, ThreadStatus::Idle);

        let status_changed = timeout(
            Duration::from_millis(250),
            read_notification_for_method(&mut ws2, "thread/status/changed"),
        )
        .await;
        assert!(
            status_changed.is_err(),
            "resident disconnect should not emit thread/status/changed -> notLoaded"
        );
        Ok(())
    }
    .await;

    process
        .kill()
        .await
        .context("failed to stop websocket app-server process")?;
    result
}

#[tokio::test]
async fn websocket_disconnect_preserves_resident_waiting_on_approval_status() -> Result<()> {
    let server =
        create_mock_responses_server_sequence_unchecked(vec![create_shell_command_sse_response(
            vec![
                "python3".to_string(),
                "-c".to_string(),
                "print(42)".to_string(),
            ],
            /*workdir*/ None,
            Some(5000),
            "call1",
        )?])
        .await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri(), "never")?;

    let (mut process, bind_addr) = spawn_websocket_server(codex_home.path()).await?;

    let result = async {
        let mut ws1 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws1, /*id*/ 1, "ws_resident_waiting_owner").await?;

        let thread = start_thread(&mut ws1, /*id*/ 10, /*resident*/ true).await?;
        start_turn_waiting_for_approval(&mut ws1, &thread.id, /*id*/ 11).await?;
        wait_for_command_execution_request_approval(&mut ws1, &thread.id).await?;

        drop(ws1);

        let mut ws2 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws2, /*id*/ 2, "ws_resident_waiting_reconnect").await?;

        let loaded_thread = loaded_read(&mut ws2, /*id*/ 12)
            .await?
            .data
            .into_iter()
            .find(|loaded_thread| loaded_thread.id == thread.id)
            .expect("thread/loaded/read should include resident waiting thread after disconnect");
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
        assert_waiting_on_approval(&loaded_thread.status);

        let read = read_thread(&mut ws2, &thread.id, /*id*/ 13).await?;
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_waiting_on_approval(&read.thread.status);

        let resumed = resume_thread(&mut ws2, &thread.id, /*id*/ 14).await?;
        assert_eq!(resumed.thread.mode, ThreadMode::ResidentAssistant);
        assert_waiting_on_approval(&resumed.thread.status);

        let status_changed = timeout(
            Duration::from_millis(250),
            read_notification_for_method(&mut ws2, "thread/status/changed"),
        )
        .await;
        assert!(
            status_changed.is_err(),
            "resident disconnect should not emit thread/status/changed -> notLoaded while waiting on approval"
        );
        Ok(())
    }
    .await;

    process
        .kill()
        .await
        .context("failed to stop websocket app-server process")?;
    result
}

#[tokio::test]
async fn websocket_disconnect_preserves_workspace_changed_after_external_change() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri(), "never")?;

    let (mut process, bind_addr) = spawn_websocket_server(codex_home.path()).await?;

    let result = async {
        let mut ws1 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws1, /*id*/ 1, "ws_resident_workspace_owner").await?;

        let thread = start_thread_with_cwd(
            &mut ws1,
            /*id*/ 10,
            /*resident*/ true,
            workspace.path(),
        )
        .await?;
        complete_turn(
            &mut ws1,
            &thread.id,
            /*id*/ 11,
            "materialize resident watcher",
        )
        .await?;

        drop(ws1);

        std::fs::write(
            workspace.path().join("watched.txt"),
            "changed after disconnect",
        )?;

        let mut ws2 = connect_websocket(bind_addr).await?;
        initialize_client(&mut ws2, /*id*/ 2, "ws_resident_workspace_reconnect").await?;

        let changed_status = wait_for_workspace_changed(&mut ws2, &thread.id, /*id*/ 12).await?;

        let loaded_thread = loaded_read(&mut ws2, /*id*/ 13)
            .await?
            .data
            .into_iter()
            .find(|loaded_thread| loaded_thread.id == thread.id)
            .expect("thread/loaded/read should include resident thread after disconnect");
        assert_eq!(loaded_thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(loaded_thread.status, changed_status);

        let read = read_thread(&mut ws2, &thread.id, /*id*/ 14).await?;
        assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(read.thread.status, changed_status);

        let resumed = resume_thread(&mut ws2, &thread.id, /*id*/ 15).await?;
        assert_eq!(resumed.thread.mode, ThreadMode::ResidentAssistant);
        assert_eq!(resumed.thread.status, changed_status);

        let status_changed = timeout(
            Duration::from_millis(250),
            read_notification_for_method(&mut ws2, "thread/status/changed"),
        )
        .await;
        assert!(
            status_changed.is_err(),
            "resident disconnect should not emit thread/status/changed after reconnect poll"
        );
        Ok(())
    }
    .await;

    process
        .kill()
        .await
        .context("failed to stop websocket app-server process")?;
    result
}

async fn initialize_client(ws: &mut WsClient, id: i64, client_name: &str) -> Result<()> {
    send_initialize_request(ws, id, client_name).await?;
    timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    Ok(())
}

async fn start_thread(
    ws: &mut WsClient,
    id: i64,
    resident: bool,
) -> Result<codex_app_server_protocol::Thread> {
    send_request(
        ws,
        "thread/start",
        id,
        Some(serde_json::to_value(ThreadStartParams {
            model: Some("mock-model".to_string()),
            resident,
            ..Default::default()
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    let started = to_response::<ThreadStartResponse>(response)?;
    Ok(started.thread)
}

async fn start_thread_with_cwd(
    ws: &mut WsClient,
    id: i64,
    resident: bool,
    cwd: &std::path::Path,
) -> Result<codex_app_server_protocol::Thread> {
    send_request(
        ws,
        "thread/start",
        id,
        Some(serde_json::to_value(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(cwd.display().to_string()),
            resident,
            ..Default::default()
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    let started = to_response::<ThreadStartResponse>(response)?;
    Ok(started.thread)
}

async fn loaded_list(ws: &mut WsClient, id: i64) -> Result<ThreadLoadedListResponse> {
    send_request(
        ws,
        "thread/loaded/list",
        id,
        Some(serde_json::to_value(ThreadLoadedListParams::default())?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    to_response::<ThreadLoadedListResponse>(response)
}

async fn loaded_read(ws: &mut WsClient, id: i64) -> Result<ThreadLoadedReadResponse> {
    send_request(
        ws,
        "thread/loaded/read",
        id,
        Some(serde_json::to_value(ThreadLoadedReadParams::default())?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    to_response::<ThreadLoadedReadResponse>(response)
}

async fn read_thread(ws: &mut WsClient, thread_id: &str, id: i64) -> Result<ThreadReadResponse> {
    send_request(
        ws,
        "thread/read",
        id,
        Some(serde_json::to_value(ThreadReadParams {
            thread_id: thread_id.to_string(),
            include_turns: false,
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    to_response::<ThreadReadResponse>(response)
}

async fn resume_thread(
    ws: &mut WsClient,
    thread_id: &str,
    id: i64,
) -> Result<ThreadResumeResponse> {
    send_request(
        ws,
        "thread/resume",
        id,
        Some(serde_json::to_value(ThreadResumeParams {
            thread_id: thread_id.to_string(),
            ..Default::default()
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    to_response::<ThreadResumeResponse>(response)
}

async fn start_turn_waiting_for_approval(
    ws: &mut WsClient,
    thread_id: &str,
    id: i64,
) -> Result<TurnStartResponse> {
    send_request(
        ws,
        "turn/start",
        id,
        Some(serde_json::to_value(TurnStartParams {
            thread_id: thread_id.to_string(),
            input: vec![UserInput::Text {
                text: "run command".to_string(),
                text_elements: Vec::new(),
            }],
            approval_policy: Some(AskForApproval::UnlessTrusted),
            ..Default::default()
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    to_response::<TurnStartResponse>(response)
}

async fn complete_turn(ws: &mut WsClient, thread_id: &str, id: i64, text: &str) -> Result<()> {
    send_request(
        ws,
        "turn/start",
        id,
        Some(serde_json::to_value(TurnStartParams {
            thread_id: thread_id.to_string(),
            input: vec![UserInput::Text {
                text: text.to_string(),
                text_elements: Vec::new(),
            }],
            model: Some("mock-model".to_string()),
            ..Default::default()
        })?),
    )
    .await?;
    let response: JSONRPCResponse =
        timeout(DEFAULT_READ_TIMEOUT, read_response_for_id(ws, id)).await??;
    let _: TurnStartResponse = to_response(response)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        read_notification_for_method(ws, "turn/completed"),
    )
    .await??;
    Ok(())
}

async fn wait_for_command_execution_request_approval(
    ws: &mut WsClient,
    thread_id: &str,
) -> Result<()> {
    loop {
        let message = super::connection_handling_websocket::read_jsonrpc_message(ws).await?;
        let JSONRPCMessage::Request(request) = message else {
            continue;
        };
        let server_request: ServerRequest = request.try_into()?;
        let ServerRequest::CommandExecutionRequestApproval { params, .. } = server_request else {
            continue;
        };
        if params.thread_id == thread_id {
            return Ok(());
        }
    }
}

fn assert_waiting_on_approval(status: &ThreadStatus) {
    assert_eq!(
        status,
        &ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WaitingOnApproval],
        }
    );
}

fn assert_workspace_changed(status: &ThreadStatus) {
    assert_eq!(
        status,
        &ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
        }
    );
}

fn parse_thread_closed(notification: JSONRPCNotification) -> Result<ThreadClosedNotification> {
    serde_json::from_value(notification.params.context("thread/closed params")?)
        .context("failed to parse thread/closed notification")
}

async fn wait_for_thread_closed_and_not_loaded(
    ws: &mut WsClient,
    thread_id: &str,
) -> Result<(ThreadClosedNotification, ThreadStatusChangedNotification)> {
    let mut closed = None;
    let mut status_changed = None;

    while closed.is_none() || status_changed.is_none() {
        let message = super::connection_handling_websocket::read_jsonrpc_message(ws).await?;
        let codex_app_server_protocol::JSONRPCMessage::Notification(notification) = message else {
            continue;
        };

        match notification.method.as_str() {
            "thread/closed" => {
                let parsed = parse_thread_closed(notification)?;
                if parsed.thread_id == thread_id {
                    closed = Some(parsed);
                }
            }
            "thread/status/changed" => {
                let parsed: ThreadStatusChangedNotification = serde_json::from_value(
                    notification
                        .params
                        .context("thread/status/changed params")?,
                )
                .context("failed to parse thread/status/changed notification")?;
                if parsed.thread_id == thread_id && parsed.status == ThreadStatus::NotLoaded {
                    status_changed = Some(parsed);
                }
            }
            _ => {}
        }
    }

    let Some(closed) = closed else {
        anyhow::bail!("missing thread/closed notification");
    };
    let Some(status_changed) = status_changed else {
        anyhow::bail!("missing thread/status/changed notification");
    };
    Ok((closed, status_changed))
}

async fn wait_for_workspace_changed(
    ws: &mut WsClient,
    thread_id: &str,
    start_id: i64,
) -> Result<ThreadStatus> {
    let mut request_id = start_id;
    timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let read = read_thread(ws, thread_id, request_id).await?;
            assert_eq!(read.thread.mode, ThreadMode::ResidentAssistant);
            if let ThreadStatus::Active { active_flags } = &read.thread.status
                && active_flags.contains(&ThreadActiveFlag::WorkspaceChanged)
            {
                assert_workspace_changed(&read.thread.status);
                return Ok::<ThreadStatus, anyhow::Error>(read.thread.status);
            }
            request_id += 1;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?
}
