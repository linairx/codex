use super::*;

pub(crate) async fn handle_thread_server_turn_start_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    thread_id: &'static str,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> bool {
    if request.method != "turn/start" {
        return false;
    }

    let requested_thread_id = request
        .params
        .as_ref()
        .and_then(|params| params.get("threadId"))
        .and_then(serde_json::Value::as_str)
        .expect("turn/start should include threadId");
    assert_eq!(requested_thread_id, thread_id);
    let turn_id = format!("turn-{thread_id}");

    write_websocket_message(
        websocket,
        JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id.clone(),
            result: serde_json::json!({
                "turn": mock_turn(&turn_id, "inProgress"),
            }),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "thread/status/changed".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "status": {
                    "type": "active",
                    "activeFlags": [],
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "turn/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turn": mock_turn(&turn_id, "inProgress"),
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "hook/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "run": {
                    "id": "hook-remote-workflow",
                    "eventName": "userPromptSubmit",
                    "handlerType": "command",
                    "executionMode": "sync",
                    "scope": "turn",
                    "sourcePath": "/tmp/remote-hooks.json",
                    "source": "user",
                    "displayOrder": 0,
                    "status": "running",
                    "statusMessage": "remote hook started",
                    "startedAt": 1,
                    "completedAt": null,
                    "durationMs": null,
                    "entries": [],
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "startedAtMs": 0,
                "item": {
                    "type": "agentMessage",
                    "id": "msg-remote-workflow",
                    "text": "streaming answer in progress",
                    "phase": "commentary",
                    "memoryCitation": null,
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "hook/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "run": {
                    "id": format!("hook-{thread_id}"),
                    "eventName": "userPromptSubmit",
                    "handlerType": "command",
                    "executionMode": "sync",
                    "scope": "turn",
                    "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                    "source": "user",
                    "displayOrder": 0,
                    "status": "running",
                    "statusMessage": format!("hook started {thread_id}"),
                    "startedAt": 1,
                    "completedAt": null,
                    "durationMs": null,
                    "entries": [],
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "startedAtMs": 0,
                "item": {
                    "type": "agentMessage",
                    "id": format!("msg-{thread_id}"),
                    "text": "streaming answer in progress",
                    "phase": "commentary",
                    "memoryCitation": null,
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/agentMessage/delta".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": format!("msg-{thread_id}"),
                "delta": "hello from recovered worker",
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/reasoning/summaryTextDelta".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": format!("reasoning-summary-{thread_id}"),
                "delta": format!("summary {thread_id}"),
                "summaryIndex": 0,
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/reasoning/textDelta".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": format!("reasoning-{thread_id}"),
                "delta": format!("reasoning {thread_id}"),
                "contentIndex": 0,
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/commandExecution/outputDelta".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": format!("exec-{thread_id}"),
                "delta": format!("stdout {thread_id}"),
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/fileChange/outputDelta".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": format!("patch-{thread_id}"),
                "delta": format!("patch {thread_id}"),
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/autoApprovalReview/started".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "startedAtMs": 0,
                "reviewId": format!("guardian-{thread_id}"),
                "targetItemId": format!("cmd-{thread_id}"),
                "review": {
                    "status": "inProgress",
                    "riskLevel": null,
                    "userAuthorization": null,
                    "rationale": null,
                },
                "action": {
                    "type": "command",
                    "source": "shell",
                    "command": "cat docs/gateway.md",
                    "cwd": "/tmp",
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/autoApprovalReview/completed".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "startedAtMs": 0,
                "completedAtMs": 0,
                "reviewId": format!("guardian-{thread_id}"),
                "targetItemId": format!("cmd-{thread_id}"),
                "decisionSource": "agent",
                "review": {
                    "status": "approved",
                    "riskLevel": "low",
                    "userAuthorization": "low",
                    "rationale": "Read-only command.",
                },
                "action": {
                    "type": "command",
                    "source": "shell",
                    "command": "cat docs/gateway.md",
                    "cwd": "/tmp",
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "hook/completed".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "run": {
                    "id": format!("hook-{thread_id}"),
                    "eventName": "userPromptSubmit",
                    "handlerType": "command",
                    "executionMode": "sync",
                    "scope": "turn",
                    "sourcePath": format!("/tmp/{thread_id}/hooks.json"),
                    "source": "user",
                    "displayOrder": 0,
                    "status": "completed",
                    "statusMessage": format!("hook completed {thread_id}"),
                    "startedAt": 1,
                    "completedAt": 2,
                    "durationMs": 1,
                    "entries": [],
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "item/completed".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "completedAtMs": 0,
                "item": {
                    "type": "agentMessage",
                    "id": format!("msg-{thread_id}"),
                    "text": "streaming answer completed",
                    "phase": "final_answer",
                    "memoryCitation": null,
                },
            })),
        }),
    )
    .await;
    write_websocket_message(
        websocket,
        JSONRPCMessage::Notification(codex_app_server_protocol::JSONRPCNotification {
            method: "turn/completed".to_string(),
            params: Some(serde_json::json!({
                "threadId": thread_id,
                "turn": mock_turn(&turn_id, "completed"),
            })),
        }),
    )
    .await;

    true
}
