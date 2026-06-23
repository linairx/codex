use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn websocket_upgrade_forwards_turn_control_requests() {
    let cases = vec![
        (
            "turn/interrupt",
            "turn-interrupt",
            serde_json::json!({
                "threadId": "thread-visible",
                "turnId": "turn-active",
            }),
            serde_json::json!({}),
        ),
        (
            "turn/steer",
            "turn-steer",
            serde_json::json!({
                "threadId": "thread-visible",
                "input": [
                    {
                        "type": "text",
                        "text": "please continue with more detail",
                        "text_elements": [],
                    }
                ],
                "responsesapiClientMetadata": null,
                "expectedTurnId": "turn-active",
            }),
            serde_json::json!({
                "turnId": "turn-active",
            }),
        ),
    ];

    for (method, request_id, params, result) in cases {
        let websocket_url = start_mock_remote_server_for_passthrough_request_with_result(
            method,
            params.clone(),
            result.clone(),
        )
        .await;
        let scope_registry = Arc::new(GatewayScopeRegistry::default());
        scope_registry.register_thread(
            "thread-visible".to_string(),
            GatewayRequestContext::default(),
        );
        let (addr, server_task) =
            spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

        let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
            .await
            .expect("websocket should connect");

        send_initialize(&mut websocket).await;
        websocket
            .send(Message::Text(
                serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                    id: RequestId::String(request_id.to_string()),
                    method: method.to_string(),
                    params: Some(params),
                    trace: None,
                }))
                .expect("request should serialize")
                .into(),
            ))
            .await
            .expect("request should send");

        let JSONRPCMessage::Response(response) = read_websocket_message(&mut websocket).await
        else {
            panic!("expected response");
        };
        assert_eq!(response.id, RequestId::String(request_id.to_string()));
        assert_eq!(response.result, result);

        server_task.abort();
        let _ = server_task.await;
    }
}

#[tokio::test]
async fn websocket_upgrade_registers_review_thread_scope_after_review_start() {
    let websocket_url = start_mock_remote_server_for_review_start_then_thread_read().await;
    let scope_registry = Arc::new(GatewayScopeRegistry::default());
    scope_registry.register_thread(
        "thread-visible".to_string(),
        GatewayRequestContext::default(),
    );
    let (addr, server_task) =
        spawn_remote_gateway_v2_test_server(websocket_url, scope_registry).await;

    let (mut websocket, _response) = connect_async(format!("ws://{addr}/"))
        .await
        .expect("websocket should connect");

    send_initialize(&mut websocket).await;
    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("review-start".to_string()),
                method: "review/start".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-visible",
                    "target": {
                        "type": "custom",
                        "instructions": "Review the current change",
                    },
                    "delivery": "detached",
                })),
                trace: None,
            }))
            .expect("review request should serialize")
            .into(),
        ))
        .await
        .expect("review request should send");

    let JSONRPCMessage::Response(review_response) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected review/start response");
    };
    assert_eq!(
        review_response.id,
        RequestId::String("review-start".to_string())
    );
    assert_eq!(
        review_response.result,
        serde_json::json!({
            "turn": {
                "id": "turn-review",
                "items": [],
                "status": "pending",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null,
            },
            "reviewThreadId": "thread-review",
        })
    );

    websocket
        .send(Message::Text(
            serde_json::to_string(&JSONRPCMessage::Request(JSONRPCRequest {
                id: RequestId::String("review-thread-read".to_string()),
                method: "thread/read".to_string(),
                params: Some(serde_json::json!({
                    "threadId": "thread-review",
                    "includeTurns": false,
                })),
                trace: None,
            }))
            .expect("thread/read request should serialize")
            .into(),
        ))
        .await
        .expect("thread/read request should send");

    let JSONRPCMessage::Response(read_response) = read_websocket_message(&mut websocket).await
    else {
        panic!("expected thread/read response");
    };
    assert_eq!(
        read_response.id,
        RequestId::String("review-thread-read".to_string())
    );
    assert_eq!(
        read_response.result,
        serde_json::json!({
            "thread": {
                "id": "thread-review",
                "name": "Detached review thread",
            },
        })
    );

    server_task.abort();
    let _ = server_task.await;
}
