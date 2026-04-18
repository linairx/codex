use std::io::Result as IoResult;

use crate::AppServerRequestHandle;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::PermissionGrantScope;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;

/// Describes how a bridge-style consumer resolved a tracked server request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadSummaryServerRequestResolution {
    CommandExecutionApproved,
    FileChangeApproved,
    PermissionsApproved,
    RequestUserInputAnswered,
    McpServerElicitationCancelled,
    RejectedUnsupported { method: &'static str },
}

pub(crate) async fn resolve_default_bridge_server_request(
    request_handle: &AppServerRequestHandle,
    request: &ServerRequest,
) -> IoResult<ThreadSummaryServerRequestResolution> {
    match request {
        ServerRequest::CommandExecutionRequestApproval { request_id, .. } => {
            request_handle
                .resolve_server_request_typed(
                    request_id.clone(),
                    &CommandExecutionRequestApprovalResponse {
                        decision: CommandExecutionApprovalDecision::Accept,
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::CommandExecutionApproved)
        }
        ServerRequest::FileChangeRequestApproval { request_id, .. } => {
            request_handle
                .resolve_server_request_typed(
                    request_id.clone(),
                    &FileChangeRequestApprovalResponse {
                        decision: FileChangeApprovalDecision::Accept,
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::FileChangeApproved)
        }
        ServerRequest::PermissionsRequestApproval { request_id, params } => {
            request_handle
                .resolve_server_request_typed(
                    request_id.clone(),
                    &PermissionsRequestApprovalResponse {
                        permissions: granted_permission_profile_from_request(&params.permissions),
                        scope: PermissionGrantScope::Turn,
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::PermissionsApproved)
        }
        ServerRequest::ToolRequestUserInput { request_id, params } => {
            request_handle
                .resolve_server_request_typed(
                    request_id.clone(),
                    &ToolRequestUserInputResponse {
                        answers: default_user_input_answers(&params.questions),
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::RequestUserInputAnswered)
        }
        ServerRequest::McpServerElicitationRequest { request_id, .. } => {
            request_handle
                .resolve_server_request_typed(
                    request_id.clone(),
                    &McpServerElicitationRequestResponse {
                        action: McpServerElicitationAction::Cancel,
                        content: None,
                        meta: None,
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::McpServerElicitationCancelled)
        }
        _ => {
            let method = server_request_method_label(request);
            request_handle
                .reject_server_request(
                    request.id().clone(),
                    JSONRPCErrorError {
                        code: -32601,
                        message: format!(
                            "bridge-style server request helper does not support `{method}`"
                        ),
                        data: None,
                    },
                )
                .await?;
            Ok(ThreadSummaryServerRequestResolution::RejectedUnsupported { method })
        }
    }
}

fn server_request_method_label(request: &ServerRequest) -> &'static str {
    match request {
        ServerRequest::CommandExecutionRequestApproval { .. } => {
            "item/commandExecution/requestApproval"
        }
        ServerRequest::FileChangeRequestApproval { .. } => "item/fileChange/requestApproval",
        ServerRequest::ToolRequestUserInput { .. } => "item/tool/requestUserInput",
        ServerRequest::McpServerElicitationRequest { .. } => "mcpServer/elicitation/request",
        ServerRequest::PermissionsRequestApproval { .. } => "item/permissions/requestApproval",
        ServerRequest::DynamicToolCall { .. } => "item/tool/call",
        ServerRequest::ChatgptAuthTokensRefresh { .. } => "account/chatgptAuthTokens/refresh",
        ServerRequest::ApplyPatchApproval { .. } => "applyPatchApproval",
        ServerRequest::ExecCommandApproval { .. } => "execCommandApproval",
    }
}

fn granted_permission_profile_from_request(
    value: &codex_app_server_protocol::RequestPermissionProfile,
) -> codex_app_server_protocol::GrantedPermissionProfile {
    codex_app_server_protocol::GrantedPermissionProfile {
        network: value.network.as_ref().map(|network| {
            codex_app_server_protocol::AdditionalNetworkPermissions {
                enabled: network.enabled,
            }
        }),
        file_system: value.file_system.as_ref().map(|file_system| {
            codex_app_server_protocol::AdditionalFileSystemPermissions {
                read: file_system.read.clone(),
                write: file_system.write.clone(),
            }
        }),
    }
}

fn default_user_input_answers(
    questions: &[ToolRequestUserInputQuestion],
) -> std::collections::HashMap<String, ToolRequestUserInputAnswer> {
    questions
        .iter()
        .map(|question| {
            let answer = question
                .options
                .as_ref()
                .and_then(|options| options.first())
                .map(|option| option.label.clone())
                .unwrap_or_default();
            (
                question.id.clone(),
                ToolRequestUserInputAnswer {
                    answers: vec![answer],
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::AppServerClient;
    use crate::RemoteAppServerClient;
    use crate::RemoteAppServerConnectArgs;
    use crate::ThreadSummaryServerRequestResolution;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::PermissionGrantScope;
    use codex_app_server_protocol::PermissionsRequestApprovalResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::RequestPermissionProfile;
    use codex_app_server_protocol::ServerRequest;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    async fn start_remote_server(request: ServerRequest) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener should have local addr");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let mut websocket = accept_async(stream)
                .await
                .expect("websocket handshake should succeed");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("initialize request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected initialize request");
            };
            let initialize_request: JSONRPCRequest =
                serde_json::from_str(&message).expect("initialize request should decode");
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: initialize_request.id,
                result: serde_json::json!({
                    "protocolVersion": 2,
                    "serverInfo": { "name": "test", "version": "0.0.0-test" }
                }),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&response)
                        .expect("initialize response should encode")
                        .into(),
                ))
                .await
                .expect("initialize response should send");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("initialized notification should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected initialized notification");
            };
            let initialized: JSONRPCNotification =
                serde_json::from_str(&message).expect("initialized notification should decode");
            assert_eq!(initialized.method, "initialized");

            let request = JSONRPCMessage::Request(
                serde_json::from_value(
                    serde_json::to_value(request).expect("request should serialize"),
                )
                .expect("request should convert to JSON-RPC"),
            );
            websocket
                .send(Message::Text(
                    serde_json::to_string(&request)
                        .expect("request should encode")
                        .into(),
                ))
                .await
                .expect("request should send");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("request resolution should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected request resolution");
            };
            let message: JSONRPCMessage =
                serde_json::from_str(&message).expect("request resolution should decode");
            match message {
                JSONRPCMessage::Response(response) => {
                    let response: PermissionsRequestApprovalResponse =
                        serde_json::from_value(response.result)
                            .expect("permissions response should decode");
                    assert_eq!(response.scope, PermissionGrantScope::Turn);
                }
                JSONRPCMessage::Error(error) => {
                    assert_eq!(error.error.code, -32601);
                }
                other => panic!("unexpected request resolution: {other:?}"),
            }
        });
        format!("ws://{addr}")
    }

    fn remote_connect_args(websocket_url: String) -> RemoteAppServerConnectArgs {
        RemoteAppServerConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "thread-summary-request-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        }
    }

    #[tokio::test]
    async fn resolve_default_bridge_server_request_handles_permissions_approvals() {
        let websocket_url = start_remote_server(ServerRequest::PermissionsRequestApproval {
            request_id: RequestId::String("srv-1".to_string()),
            params: codex_app_server_protocol::PermissionsRequestApprovalParams {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                item_id: "item-1".to_string(),
                reason: Some("need access".to_string()),
                permissions: RequestPermissionProfile {
                    network: None,
                    file_system: None,
                },
            },
        })
        .await;
        let mut client = AppServerClient::Remote(
            RemoteAppServerClient::connect(remote_connect_args(websocket_url))
                .await
                .expect("remote client should connect"),
        );

        let crate::AppServerEvent::ServerRequest(request) = client
            .next_event()
            .await
            .expect("server request should arrive")
        else {
            panic!("expected server request event");
        };
        let resolution =
            super::resolve_default_bridge_server_request(&client.request_handle(), &request)
                .await
                .expect("request should resolve");
        assert_eq!(
            resolution,
            ThreadSummaryServerRequestResolution::PermissionsApproved
        );
    }
}
