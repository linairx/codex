use super::*;
use pretty_assertions::assert_eq;

pub(crate) async fn start_mock_remote_server_for_realtime_start() -> String {
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

        let frame = websocket
            .next()
            .await
            .expect("realtime request should exist")
            .expect("realtime request should decode");
        let Message::Text(text) = frame else {
            panic!("expected realtime request text frame");
        };
        let JSONRPCMessage::Request(request) =
            serde_json::from_str(&text).expect("realtime request should decode")
        else {
            panic!("expected realtime request");
        };
        assert_eq!(request.method, "thread/realtime/start");
        assert_json_params_eq(
            request.params,
            Some(serde_json::json!({
                "threadId": "thread-visible",
                "outputModality": "text",
                "realtimeSessionId": null,
                "transport": {
                    "type": "websocket"
                },
                "voice": null,
            })),
        );

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: serde_json::json!({}),
                }))
                .expect("realtime response should serialize")
                .into(),
            ))
            .await
            .expect("realtime response should send");

        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_realtime_append_text() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/appendText",
        serde_json::json!({
            "threadId": "thread-visible",
            "text": "hello realtime",
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_append_audio() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/appendAudio",
        serde_json::json!({
            "threadId": "thread-visible",
            "audio": {
                "data": "AQID",
                "sampleRate": 24000,
                "numChannels": 1,
                "samplesPerChannel": 3,
                "itemId": "item-visible",
            }
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_stop() -> String {
    start_mock_remote_server_for_passthrough_request(
        "thread/realtime/stop",
        serde_json::json!({
            "threadId": "thread-visible",
        }),
    )
    .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_started_notification() -> String {
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

        send_remote_notification(
            &mut websocket,
            "thread/realtime/started",
            serde_json::json!({
                "threadId": "thread-visible",
                "realtimeSessionId": "realtime-session-1",
                "version": "v1",
            }),
        )
        .await;

        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_realtime_notification(
    method: &str,
    params: serde_json::Value,
) -> String {
    start_mock_remote_server_for_realtime_notifications(vec![(method, params)]).await
}

pub(crate) async fn start_mock_remote_server_for_realtime_notifications(
    notifications: Vec<(&str, serde_json::Value)>,
) -> String {
    start_mock_remote_server_for_realtime_notifications_after_delay(notifications, Duration::ZERO)
        .await
}

pub(crate) async fn start_mock_remote_server_for_realtime_notifications_after_delay(
    notifications: Vec<(&str, serde_json::Value)>,
    delay: Duration,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let addr = listener.local_addr().expect("listener address");
    let notifications = notifications
        .into_iter()
        .map(|(method, params)| (method.to_string(), params))
        .collect::<Vec<_>>();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept should succeed");
        let mut websocket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket should accept");

        expect_remote_initialize(&mut websocket).await;
        tokio::time::sleep(delay).await;
        for (method, params) in notifications {
            send_remote_notification(&mut websocket, &method, params).await;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request(
    expected_method: &'static str,
    expected_params: serde_json::Value,
) -> String {
    start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
        expected_method,
        Some(expected_params),
        serde_json::json!({}),
    )
    .await
}

pub(crate) fn canonicalize_json_params(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("version")
                && map.contains_key("threadId")
                && let Some(session_id) = map.remove("sessionId")
            {
                map.entry("realtimeSessionId".to_string())
                    .or_insert(session_id);
            }
            map.retain(|key, value| {
                if value.is_null() {
                    return false;
                }
                if matches!(
                    key.as_str(),
                    "refreshToken"
                        | "experimentalRawEvents"
                        | "persistExtendedHistory"
                        | "includeLogs"
                        | "useStateDbOnly"
                ) && value == &serde_json::Value::Bool(false)
                {
                    return false;
                }
                if key == "includeTurns" && value == &serde_json::Value::Bool(false) {
                    return false;
                }
                if key == "role" && value == &serde_json::Value::String("user".to_string()) {
                    return false;
                }
                true
            });
            for value in map.values_mut() {
                canonicalize_json_params(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize_json_params(value);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

pub(crate) fn canonicalize_bootstrap_response_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("installPolicy")
                && map.contains_key("authPolicy")
                && map.contains_key("interface")
            {
                if map.get("availability")
                    == Some(&serde_json::Value::String("AVAILABLE".to_string()))
                {
                    map.remove("availability");
                }
                if map.get("keywords") == Some(&serde_json::Value::Array(Vec::new())) {
                    map.remove("keywords");
                }
                if map
                    .get("localVersion")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("localVersion");
                }
                if map
                    .get("remotePluginId")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("remotePluginId");
                }
                if map
                    .get("shareContext")
                    .is_some_and(serde_json::Value::is_null)
                {
                    map.remove("shareContext");
                }
            }
            if map.contains_key("authStatus")
                && map.contains_key("resourceTemplates")
                && map.contains_key("resources")
                && map.contains_key("tools")
                && map
                    .get("serverInfo")
                    .is_some_and(serde_json::Value::is_null)
            {
                map.remove("serverInfo");
            }
            for value in map.values_mut() {
                canonicalize_bootstrap_response_json(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize_bootstrap_response_json(value);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

pub(crate) fn assert_json_params_eq(
    actual: Option<serde_json::Value>,
    expected: Option<serde_json::Value>,
) {
    let mut actual = actual;
    let mut expected = expected;
    if let Some(actual) = &mut actual {
        canonicalize_json_params(actual);
    }
    if let Some(expected) = &mut expected {
        canonicalize_json_params(expected);
    }
    assert_eq!(actual, expected);
}

pub(crate) async fn start_mock_remote_server_for_single_request(
    request_tx: oneshot::Sender<JSONRPCRequest>,
    response_result: serde_json::Value,
) -> String {
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

        let request = read_websocket_request(&mut websocket).await;
        request_tx
            .send(request.clone())
            .expect("request should be captured");

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id,
                    result: response_result,
                }))
                .expect("response should serialize")
                .into(),
            ))
            .await
            .expect("response should send");

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_result(
    expected_method: &'static str,
    expected_params: serde_json::Value,
    response_result: serde_json::Value,
) -> String {
    start_mock_remote_server_for_passthrough_request_with_optional_params_and_result(
        expected_method,
        Some(expected_params),
        response_result,
    )
    .await
}

pub(crate) fn plugin_summary_json(
    id: &str,
    installed: bool,
    enabled: bool,
    short_description: &str,
) -> PluginSummary {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": id.strip_suffix("@local").unwrap_or(id),
        "source": {
            "type": "local",
            "path": format!("/tmp/project/plugins/{id}"),
        },
        "installed": installed,
        "enabled": enabled,
        "availability": "AVAILABLE",
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_USE",
        "keywords": [],
        "localVersion": null,
        "remotePluginId": null,
        "shareContext": null,
        "interface": {
            "displayName": id,
            "shortDescription": short_description,
            "longDescription": null,
            "developerName": null,
            "category": null,
            "capabilities": [],
            "websiteUrl": null,
            "privacyPolicyUrl": null,
            "termsOfServiceUrl": null,
            "defaultPrompt": null,
            "brandColor": null,
            "composerIcon": null,
            "composerIconUrl": null,
            "logo": null,
            "logoUrl": null,
            "screenshots": [],
            "screenshotUrls": [],
        },
    }))
    .expect("plugin summary should deserialize")
}

pub(crate) async fn start_mock_remote_server_for_paginated_passthrough_requests(
    expected_method: &'static str,
    expected_requests_and_results: Vec<(serde_json::Value, serde_json::Value)>,
) -> String {
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

        for (expected_params, response_result) in expected_requests_and_results {
            let request = read_websocket_request(&mut websocket).await;
            assert_eq!(request.method, expected_method);
            assert_json_params_eq(request.params, Some(expected_params));

            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: response_result,
                    }))
                    .expect("response should serialize")
                    .into(),
                ))
                .await
                .expect("response should send");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    format!("ws://{addr}")
}

pub(crate) async fn start_mock_remote_server_for_passthrough_request_with_error(
    expected_method: &'static str,
    expected_params: serde_json::Value,
    response_error: JSONRPCErrorError,
) -> String {
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

        let request = read_websocket_request(&mut websocket).await;
        assert_eq!(request.method, expected_method);
        assert_json_params_eq(request.params, Some(expected_params));

        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Error(JSONRPCError {
                    id: request.id,
                    error: response_error,
                }))
                .expect("error response should serialize")
                .into(),
            ))
            .await
            .expect("error response should send");
    });
    format!("ws://{addr}")
}
