use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_skills_changed_and_list(
    cwd: &str,
    skills: Vec<&str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let cwd = cwd.to_string();
    let skills = skills
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": format!("{name} description"),
                "path": format!("{cwd}/{name}"),
                "scope": "repo",
                "enabled": true,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        loop {
            let Some(frame) = websocket.next().await else {
                break;
            };
            let frame = frame.expect("frame should decode");
            let Message::Text(text) = frame else {
                continue;
            };
            let JSONRPCMessage::Request(request) =
                serde_json::from_str(&text).expect("request should decode")
            else {
                continue;
            };
            assert_eq!(request.method, "skills/list");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::json!({
                            "data": [{
                                "cwd": cwd.clone(),
                                "skills": skills.clone(),
                                "errors": [],
                            }]
                        }),
                    }))
                    .expect("skills/list response should serialize")
                    .into(),
                ))
                .await
                .expect("skills/list response should send");
            send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        }
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_skills_changed_and_failing_list() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: request.id,
                    error: JSONRPCErrorError {
                        code: super::super::super::INTERNAL_ERROR_CODE,
                        message: "skills list failed".to_string(),
                        data: None,
                    },
                }))
                .expect("skills/list error should serialize")
                .into(),
            ))
            .await
            .expect("skills/list error should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_skills_changed_failing_then_successful_list()
-> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let failed_request = read_websocket_request(&mut websocket).await;
        assert_eq!(failed_request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: failed_request.id,
                    error: JSONRPCErrorError {
                        code: super::super::super::INTERNAL_ERROR_CODE,
                        message: "first skills list failed".to_string(),
                        data: None,
                    },
                }))
                .expect("skills/list error should serialize")
                .into(),
            ))
            .await
            .expect("skills/list error should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;

        let successful_request = read_websocket_request(&mut websocket).await;
        assert_eq!(successful_request.method, "skills/list");
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: successful_request.id,
                    result: serde_json::json!({
                        "data": [{
                            "cwd": "/tmp/worker-a",
                            "skills": [{
                                "name": "skill-a",
                                "description": "skill-a description",
                                "path": "/tmp/worker-a/skill-a",
                                "scope": "repo",
                                "enabled": true,
                            }],
                            "errors": [],
                        }]
                    }),
                }))
                .expect("skills/list response should serialize")
                .into(),
            ))
            .await
            .expect("skills/list response should send");
        send_remote_notification(&mut websocket, "skills/changed", serde_json::json!({})).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_reconnectable_skills_list(
    cwd: &str,
    skills: Vec<&str>,
) -> String {
    let cwd = cwd.to_string();
    let skills = skills
        .into_iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "description": format!("{name} description"),
                "path": format!("{cwd}/{name}"),
                "scope": "repo",
                "enabled": true,
            })
        })
        .collect::<Vec<serde_json::Value>>();
    start_mock_remote_server_for_reconnectable_request(
        "skills/list",
        serde_json::json!({
            "data": [{
                "cwd": cwd,
                "skills": skills,
                "errors": [],
            }]
        }),
    )
    .await
}
