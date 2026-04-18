use std::io::Result as IoResult;

use crate::AppServerClient;
use crate::AppServerRequestHandle;
use crate::ThreadSummaryBootstrapParams;
use crate::ThreadSummaryBootstrapResult;
use crate::ThreadSummaryServerRequestResolution;
use crate::ThreadSummaryTrackedEvent;
use crate::ThreadSummaryTracker;
use crate::TypedRequestError;
use crate::thread_summary_server_request::resolve_default_bridge_server_request;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::Result as JsonRpcResult;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use serde::Serialize;

/// High-level retained-summary session for bridge-style app-server consumers.
///
/// This wrapper keeps one [`ThreadSummaryTracker`] aligned with the underlying
/// [`AppServerClient`] so callers can bootstrap `thread/list` /
/// `thread/loaded/read` once and then continue consuming tracked lifecycle
/// events from `next_tracked_event()`.
pub struct ThreadSummaryClient {
    client: AppServerClient,
    request_handle: AppServerRequestHandle,
    tracker: ThreadSummaryTracker,
}

impl ThreadSummaryClient {
    pub fn new(client: AppServerClient) -> Self {
        let request_handle = client.request_handle();
        Self {
            client,
            request_handle,
            tracker: ThreadSummaryTracker::new(),
        }
    }

    pub fn with_tracker(client: AppServerClient, tracker: ThreadSummaryTracker) -> Self {
        let request_handle = client.request_handle();
        Self {
            client,
            request_handle,
            tracker,
        }
    }

    pub fn tracker(&self) -> &ThreadSummaryTracker {
        &self.tracker
    }

    pub fn tracker_mut(&mut self) -> &mut ThreadSummaryTracker {
        &mut self.tracker
    }

    pub fn request_handle(&self) -> &AppServerRequestHandle {
        &self.request_handle
    }

    pub fn client(&self) -> &AppServerClient {
        &self.client
    }

    pub fn into_parts(self) -> (AppServerClient, ThreadSummaryTracker) {
        (self.client, self.tracker)
    }

    pub async fn bootstrap_all_pages(
        &mut self,
        params: ThreadSummaryBootstrapParams,
    ) -> Result<ThreadSummaryBootstrapResult, TypedRequestError> {
        self.tracker
            .bootstrap_all_pages(&self.request_handle, params)
            .await
    }

    pub async fn fetch_thread_list_page(
        &mut self,
        params: ThreadListParams,
    ) -> Result<ThreadListResponse, TypedRequestError> {
        self.tracker
            .fetch_thread_list_page(&self.request_handle, params)
            .await
    }

    pub async fn fetch_thread_loaded_read_page(
        &mut self,
        params: ThreadLoadedReadParams,
    ) -> Result<ThreadLoadedReadResponse, TypedRequestError> {
        self.tracker
            .fetch_thread_loaded_read_page(&self.request_handle, params)
            .await
    }

    pub async fn next_tracked_event(&mut self) -> Option<ThreadSummaryTrackedEvent> {
        let event = self.client.next_event().await?;
        Some(self.tracker.track_event(&self.request_handle, event).await)
    }

    pub async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: JsonRpcResult,
    ) -> IoResult<()> {
        self.request_handle
            .resolve_server_request(request_id, result)
            .await
    }

    pub async fn resolve_server_request_typed<T>(
        &self,
        request_id: RequestId,
        result: &T,
    ) -> IoResult<()>
    where
        T: Serialize,
    {
        self.request_handle
            .resolve_server_request_typed(request_id, result)
            .await
    }

    pub async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> IoResult<()> {
        self.request_handle
            .reject_server_request(request_id, error)
            .await
    }

    pub async fn resolve_default_bridge_server_request(
        &self,
        request: &codex_app_server_protocol::ServerRequest,
    ) -> IoResult<ThreadSummaryServerRequestResolution> {
        resolve_default_bridge_server_request(&self.request_handle, request).await
    }

    pub async fn shutdown(self) -> IoResult<()> {
        self.client.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadSummaryClient;
    use crate::AppServerClient;
    use crate::RemoteAppServerClient;
    use crate::RemoteAppServerConnectArgs;
    use crate::ThreadSummaryBootstrapParams;
    use crate::ThreadSummaryTrackedEvent;
    use codex_app_server_protocol::ClientRequest;
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
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedReadParams;
    use codex_app_server_protocol::ThreadLoadedReadResponse;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadStatus;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    fn remote_connect_args(websocket_url: String) -> RemoteAppServerConnectArgs {
        RemoteAppServerConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "codex-app-server-client-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        }
    }

    fn make_thread(id: &str, name: Option<&str>, status: ThreadStatus) -> Thread {
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
            name: name.map(ToString::to_string),
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
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("initialize request should decode");
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
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

            let resident = make_thread(
                "thread-1",
                Some("atlas"),
                ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                },
            );
            let loaded = make_thread("thread-2", Some("loaded"), ThreadStatus::Idle);

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
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::to_value(ThreadListResponse {
                    data: vec![resident.clone()],
                    next_cursor: None,
                })
                .expect("thread/list response should encode"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&response)
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
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![loaded],
                    next_cursor: None,
                })
                .expect("thread/loaded/read response should encode"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&response)
                        .expect("thread/loaded/read response should encode")
                        .into(),
                ))
                .await
                .expect("thread/loaded/read response should send");

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
    async fn bootstrap_and_next_tracked_event_share_one_retained_summary_session() {
        let websocket_url = start_remote_server().await;
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let mut client = ThreadSummaryClient::new(AppServerClient::Remote(client));

        let result = client
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

        assert_eq!(result.thread_list_threads, 1);
        assert_eq!(result.thread_loaded_read_threads, 1);
        assert_eq!(client.tracker().iter().count(), 2);

        let Some(ThreadSummaryTrackedEvent::ThreadClosed { summary, .. }) =
            client.next_tracked_event().await
        else {
            panic!("expected tracked thread/closed event");
        };

        let summary = summary.expect("thread/closed should retain cached summary");
        assert_eq!(summary.id, "thread-1");
        assert_eq!(summary.name.as_deref(), Some("atlas"));
        assert_eq!(summary.mode, ThreadMode::ResidentAssistant);
        assert_eq!(summary.status, ThreadStatus::NotLoaded);
    }

    #[tokio::test]
    async fn tracked_server_requests_can_be_resolved_through_thread_summary_client() {
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
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("initialize request should decode");
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
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
                    serde_json::to_value(ServerRequest::PermissionsRequestApproval {
                        request_id: codex_app_server_protocol::RequestId::String(
                            "srv-1".to_string(),
                        ),
                        params: codex_app_server_protocol::PermissionsRequestApprovalParams {
                            thread_id: "thread-1".to_string(),
                            turn_id: "turn-1".to_string(),
                            item_id: "item-1".to_string(),
                            permissions: RequestPermissionProfile {
                                network: None,
                                file_system: None,
                            },
                            reason: None,
                        },
                    })
                    .expect("permissions request should encode"),
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
                .expect("server request response should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected server request response");
            };
            let response: JSONRPCResponse =
                serde_json::from_str(&message).expect("server request response should decode");
            assert_eq!(
                response.id,
                codex_app_server_protocol::RequestId::String("srv-1".to_string())
            );
            let response: PermissionsRequestApprovalResponse =
                serde_json::from_value(response.result)
                    .expect("permissions response should decode");
            assert_eq!(response.scope, PermissionGrantScope::Turn);
        });

        let websocket_url = format!("ws://{addr}");
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let mut client = ThreadSummaryClient::new(AppServerClient::Remote(client));

        let Some(ThreadSummaryTrackedEvent::ServerRequest { request, .. }) =
            client.next_tracked_event().await
        else {
            panic!("expected tracked server request event");
        };

        let ServerRequest::PermissionsRequestApproval { request_id, .. } = request else {
            panic!("expected permissions approval request");
        };

        client
            .resolve_server_request_typed(
                request_id,
                &PermissionsRequestApprovalResponse {
                    permissions: codex_app_server_protocol::GrantedPermissionProfile {
                        network: None,
                        file_system: None,
                    },
                    scope: PermissionGrantScope::Turn,
                },
            )
            .await
            .expect("typed server request resolution should succeed");
    }

    #[tokio::test]
    async fn helper_requests_use_distinct_request_ids_from_callers() {
        let (manual_request_seen_tx, manual_request_seen_rx) = oneshot::channel();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener should have local addr");
        tokio::spawn(async move {
            let mut manual_request_seen_tx = Some(manual_request_seen_tx);
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
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("initialize request should decode");
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
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

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("manual thread/read request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected manual thread/read request");
            };
            let thread_read_request: JSONRPCRequest =
                serde_json::from_str(&message).expect("thread/read request should decode");
            assert_eq!(thread_read_request.method, "thread/read");
            assert_eq!(thread_read_request.id, RequestId::Integer(1));
            manual_request_seen_tx
                .take()
                .expect("manual request signal should be present")
                .send(())
                .expect("manual request signal should send");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("helper thread/list request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected helper thread/list request");
            };
            let thread_list_request: JSONRPCRequest =
                serde_json::from_str(&message).expect("thread/list request should decode");
            assert_eq!(thread_list_request.method, "thread/list");
            assert_ne!(thread_list_request.id, RequestId::Integer(1));

            let thread_list_response = JSONRPCMessage::Response(JSONRPCResponse {
                id: thread_list_request.id,
                result: serde_json::to_value(ThreadListResponse {
                    data: vec![make_thread("thread-1", Some("atlas"), ThreadStatus::Idle)],
                    next_cursor: None,
                })
                .expect("thread/list response should encode"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&thread_list_response)
                        .expect("thread/list response should encode")
                        .into(),
                ))
                .await
                .expect("thread/list response should send");

            let thread_read_response = JSONRPCMessage::Response(JSONRPCResponse {
                id: thread_read_request.id,
                result: serde_json::to_value(ThreadReadResponse {
                    thread: make_thread("thread-1", Some("atlas"), ThreadStatus::Idle),
                })
                .expect("thread/read response should encode"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&thread_read_response)
                        .expect("thread/read response should encode")
                        .into(),
                ))
                .await
                .expect("thread/read response should send");
        });

        let websocket_url = format!("ws://{addr}");
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let mut client = ThreadSummaryClient::new(AppServerClient::Remote(client));
        let request_handle = client.request_handle().clone();
        let pending_thread_read = tokio::spawn(async move {
            request_handle
                .request_typed::<ThreadReadResponse>(ClientRequest::ThreadRead {
                    request_id: RequestId::Integer(1),
                    params: ThreadReadParams {
                        thread_id: "thread-1".to_string(),
                        include_turns: false,
                    },
                })
                .await
        });
        manual_request_seen_rx
            .await
            .expect("manual thread/read request should be observed before helper request");

        let thread_list = client
            .fetch_thread_list_page(ThreadListParams {
                cursor: None,
                limit: Some(20),
                sort_key: None,
                model_providers: None,
                source_kinds: None,
                archived: None,
                cwd: None,
                search_term: None,
            })
            .await
            .expect("thread summary helper should not collide with caller request ids");

        assert_eq!(thread_list.data.len(), 1);
        let thread_read = pending_thread_read
            .await
            .expect("thread/read task should join")
            .expect("thread/read request should succeed");
        assert_eq!(thread_read.thread.id, "thread-1");
    }
}
