use super::*;

#[allow(dead_code)]
pub(crate) const RECONNECT_NOTIFICATION_DELAY: Duration = Duration::from_secs(1);

#[allow(dead_code)]
pub(crate) async fn respond_to_reconnect_bootstrap_request(
    websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    request: &codex_app_server_protocol::JSONRPCRequest,
) -> bool {
    match request.method.as_str() {
        "account/read" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "account": {
                            "type": "chatgpt",
                            "email": "gateway@example.com",
                            "planType": "pro",
                        },
                        "requiresOpenaiAuth": false,
                    }),
                }),
            )
            .await;
            true
        }
        "account/rateLimits/read" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "rateLimits": {
                            "limitId": "codex",
                            "limitName": "Codex",
                            "primary": {
                                "usedPercent": 42,
                                "windowDurationMins": 300,
                                "resetsAt": 1_700_000_000,
                            },
                            "secondary": null,
                            "credits": null,
                            "planType": "pro",
                            "rateLimitReachedType": null,
                        },
                        "rateLimitsByLimitId": {
                            "codex": {
                                "limitId": "codex",
                                "limitName": "Codex",
                                "primary": {
                                    "usedPercent": 42,
                                    "windowDurationMins": 300,
                                    "resetsAt": 1_700_000_000,
                                },
                                "secondary": null,
                                "credits": null,
                                "planType": "pro",
                                "rateLimitReachedType": null,
                            }
                        },
                    }),
                }),
            )
            .await;
            true
        }
        "app/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [{
                            "id": "calendar",
                            "name": "calendar",
                            "description": null,
                            "logoUrl": null,
                            "logoUrlDark": null,
                            "distributionChannel": null,
                            "branding": null,
                            "appMetadata": null,
                            "labels": null,
                            "installUrl": null,
                        }],
                        "nextCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "configRequirements/read" => {
            assert_eq!(request.params, None);
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "requirements": null,
                    }),
                }),
            )
            .await;
            true
        }
        "mcpServerStatus/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [{
                            "name": "calendar-mcp",
                            "tools": {},
                            "resources": [],
                            "resourceTemplates": [],
                            "authStatus": "bearerToken",
                        }],
                        "nextCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "model/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [{
                            "id": "openai/gpt-5",
                            "model": "gpt-5",
                            "upgrade": null,
                            "upgradeInfo": null,
                            "availabilityNux": null,
                            "displayName": "GPT-5",
                            "description": "Recovered single-worker model",
                            "hidden": false,
                            "supportedReasoningEfforts": [{
                                "reasoningEffort": "medium",
                                "description": "Balanced",
                            }],
                            "defaultReasoningEffort": "medium",
                            "inputModalities": ["text"],
                            "supportsPersonality": false,
                            "additionalSpeedTiers": [],
                            "isDefault": true,
                        }],
                        "nextCursor": null,
                    }),
                }),
            )
            .await;
            true
        }
        "skills/list" => {
            write_websocket_message(
                websocket,
                JSONRPCMessage::Response(JSONRPCResponse {
                    id: request.id.clone(),
                    result: serde_json::json!({
                        "data": [{
                            "cwd": "/tmp/shared-repo",
                            "skills": [],
                            "errors": [],
                        }],
                    }),
                }),
            )
            .await;
            true
        }
        _ => false,
    }
}
