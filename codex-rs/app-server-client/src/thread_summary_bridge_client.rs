use std::io::Result as IoResult;

use crate::AppServerClient;
use crate::ThreadSummaryBootstrapParams;
use crate::ThreadSummaryBootstrapResult;
use crate::ThreadSummaryClient;
use crate::ThreadSummaryServerRequestResolution;
use crate::ThreadSummaryTrackedEvent;
use crate::ThreadSummaryTracker;
use crate::TypedRequestError;

/// One bridge-facing event from a retained-summary session.
///
/// This keeps the tracked thread event together with the default bridge-side
/// server-request action that may have been sent back to app-server.
#[derive(Debug)]
pub struct ThreadSummaryBridgeEvent {
    pub tracked_event: ThreadSummaryTrackedEvent,
    pub server_request_resolution: Option<ThreadSummaryServerRequestResolution>,
}

/// High-level bridge consumer facade built on top of [`ThreadSummaryClient`].
///
/// It keeps the retained thread-summary cache, exposes the same bootstrap
/// entrypoints, and automatically applies the current default bridge policy for
/// server requests while still returning the underlying tracked event.
pub struct ThreadSummaryBridgeClient {
    summary_client: ThreadSummaryClient,
}

impl ThreadSummaryBridgeClient {
    pub fn new(client: AppServerClient) -> Self {
        Self {
            summary_client: ThreadSummaryClient::new(client),
        }
    }

    pub fn with_tracker(client: AppServerClient, tracker: ThreadSummaryTracker) -> Self {
        Self {
            summary_client: ThreadSummaryClient::with_tracker(client, tracker),
        }
    }

    pub fn summary_client(&self) -> &ThreadSummaryClient {
        &self.summary_client
    }

    pub fn summary_client_mut(&mut self) -> &mut ThreadSummaryClient {
        &mut self.summary_client
    }

    pub fn tracker(&self) -> &ThreadSummaryTracker {
        self.summary_client.tracker()
    }

    pub fn tracker_mut(&mut self) -> &mut ThreadSummaryTracker {
        self.summary_client.tracker_mut()
    }

    pub async fn bootstrap_all_pages(
        &mut self,
        params: ThreadSummaryBootstrapParams,
    ) -> Result<ThreadSummaryBootstrapResult, TypedRequestError> {
        self.summary_client.bootstrap_all_pages(params).await
    }

    pub async fn next_bridge_event(&mut self) -> Option<IoResult<ThreadSummaryBridgeEvent>> {
        let tracked_event = self.summary_client.next_tracked_event().await?;
        let server_request_resolution = match &tracked_event {
            ThreadSummaryTrackedEvent::ServerRequest { request, .. } => Some(
                self.summary_client
                    .resolve_default_bridge_server_request(request)
                    .await,
            ),
            _ => None,
        };

        let server_request_resolution = match server_request_resolution {
            Some(Ok(resolution)) => Some(resolution),
            Some(Err(err)) => return Some(Err(err)),
            None => None,
        };

        Some(Ok(ThreadSummaryBridgeEvent {
            tracked_event,
            server_request_resolution,
        }))
    }

    pub async fn shutdown(self) -> IoResult<()> {
        self.summary_client.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadSummaryBridgeClient;
    use crate::AppServerClient;
    use crate::RemoteAppServerClient;
    use crate::RemoteAppServerConnectArgs;
    use crate::ThreadSummaryBootstrapParams;
    use crate::ThreadSummaryBridgeEvent;
    use crate::ThreadSummaryServerRequestResolution;
    use crate::ThreadSummaryTrackedEvent;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::PermissionGrantScope;
    use codex_app_server_protocol::PermissionsRequestApprovalResponse;
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::RequestPermissionProfile;
    use codex_app_server_protocol::ServerRequest;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedReadParams;
    use codex_app_server_protocol::ThreadLoadedReadResponse;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadStatus;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    fn remote_connect_args(websocket_url: String) -> RemoteAppServerConnectArgs {
        RemoteAppServerConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "thread-summary-bridge-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        }
    }

    fn make_thread(id: &str, status: ThreadStatus) -> Thread {
        Thread {
            id: id.to_string(),
            forked_from_id: None,
            preview: "preview".to_string(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at: 1,
            updated_at: 2,
            status,
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            path: None,
            cwd: PathBuf::from("/tmp/atlas"),
            cli_version: "1.0.0".to_string(),
            source: SessionSource::Cli,
            agent_nickname: Some("Atlas".to_string()),
            agent_role: Some("worker".to_string()),
            git_info: None,
            name: Some("atlas".to_string()),
            turns: Vec::new(),
        }
    }

    async fn start_remote_server() -> String {
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

            let thread_list_result = serde_json::to_value(ThreadListResponse {
                data: vec![make_thread("thread-1", ThreadStatus::Idle)],
                next_cursor: None,
            })
            .expect("thread/list response should encode");
            let Message::Text(message) = websocket
                .next()
                .await
                .expect("thread/list request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected thread/list request");
            };
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("thread/list request should decode");
            assert_eq!(request.method, "thread/list");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: thread_list_result,
                    }))
                    .expect("thread/list response should encode")
                    .into(),
                ))
                .await
                .expect("thread/list response should send");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("thread/loaded/read request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected thread/loaded/read request");
            };
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("thread/loaded/read request should decode");
            assert_eq!(request.method, "thread/loaded/read");
            websocket
                .send(Message::Text(
                    serde_json::to_string(&JSONRPCMessage::Response(JSONRPCResponse {
                        id: request.id,
                        result: serde_json::to_value(ThreadLoadedReadResponse {
                            data: vec![make_thread("thread-1", ThreadStatus::Idle)],
                            next_cursor: None,
                        })
                        .expect("thread/loaded/read response should encode"),
                    }))
                    .expect("thread/loaded/read response should encode")
                    .into(),
                ))
                .await
                .expect("thread/loaded/read response should send");

            let request = JSONRPCMessage::Request(
                serde_json::from_value(
                    serde_json::to_value(ServerRequest::PermissionsRequestApproval {
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
                    .expect("permissions request should serialize"),
                )
                .expect("permissions request should convert to JSON-RPC"),
            );
            websocket
                .send(Message::Text(
                    serde_json::to_string(&request)
                        .expect("permissions request should encode")
                        .into(),
                ))
                .await
                .expect("permissions request should send");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("permissions response should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected permissions response");
            };
            let response: JSONRPCResponse =
                serde_json::from_str(&message).expect("permissions response should decode");
            let response: PermissionsRequestApprovalResponse =
                serde_json::from_value(response.result)
                    .expect("permissions response should decode");
            assert_eq!(response.scope, PermissionGrantScope::Turn);

            let notification = JSONRPCMessage::Notification(JSONRPCNotification {
                method: "thread/closed".to_string(),
                params: Some(
                    serde_json::to_value(ThreadClosedNotification {
                        thread_id: "thread-1".to_string(),
                    })
                    .expect("thread/closed notification should encode"),
                ),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&notification)
                        .expect("thread/closed notification should encode")
                        .into(),
                ))
                .await
                .expect("thread/closed notification should send");
        });
        format!("ws://{addr}")
    }

    #[tokio::test]
    async fn next_bridge_event_resolves_default_bridge_requests() {
        let websocket_url = start_remote_server().await;
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let mut client = ThreadSummaryBridgeClient::new(AppServerClient::Remote(client));

        client
            .bootstrap_all_pages(ThreadSummaryBootstrapParams {
                thread_list: ThreadListParams {
                    cursor: None,
                    limit: Some(20),
                    sort_key: None,
                    model_providers: None,
                    source_kinds: None,
                    archived: None,
                    cwd: None,
                    search_term: None,
                },
                thread_loaded_read: ThreadLoadedReadParams {
                    cursor: None,
                    limit: Some(20),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            })
            .await
            .expect("bootstrap should succeed");

        let Some(Ok(ThreadSummaryBridgeEvent {
            tracked_event:
                ThreadSummaryTrackedEvent::ServerRequest {
                    thread_id, summary, ..
                },
            server_request_resolution,
        })) = client.next_bridge_event().await
        else {
            panic!("expected resolved bridge server request event");
        };
        assert_eq!(thread_id.as_deref(), Some("thread-1"));
        assert_eq!(summary.expect("summary should be present").id, "thread-1");
        assert_eq!(
            server_request_resolution,
            Some(ThreadSummaryServerRequestResolution::PermissionsApproved)
        );
    }
}
