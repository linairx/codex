use crate::AppServerEvent;
use crate::AppServerRequestHandle;
use crate::ThreadSummaryTracker;
use crate::ThreadSummaryUpdate;
use crate::TypedRequestError;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::FileChangeRequestApprovalParams;
use codex_app_server_protocol::McpServerElicitationRequestParams;
use codex_app_server_protocol::PermissionsRequestApprovalParams;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadArchivedNotification;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadNameUpdatedNotification;
use codex_app_server_protocol::ThreadStartedNotification;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_app_server_protocol::ThreadUnarchivedNotification;

/// Outcome of the optional `thread/read` refresh triggered by a lifecycle edge.
#[derive(Debug)]
pub enum ThreadSummaryRefresh {
    NotNeeded,
    Refreshed(Box<Thread>),
    Failed {
        thread_id: String,
        error: TypedRequestError,
    },
}

/// Structured thread-summary event stream for bridge-style consumers.
#[derive(Debug)]
pub enum ThreadSummaryTrackedEvent {
    Lagged {
        skipped: usize,
    },
    ServerRequest {
        request: ServerRequest,
        thread_id: Option<String>,
        summary: Option<Box<Thread>>,
    },
    Disconnected {
        message: String,
    },
    ThreadStarted {
        payload: ThreadStartedNotification,
        summary: Box<Thread>,
    },
    ThreadStatusChanged {
        payload: ThreadStatusChangedNotification,
        summary: Option<Box<Thread>>,
        refresh: ThreadSummaryRefresh,
        update: ThreadSummaryUpdate,
    },
    ThreadNameUpdated {
        payload: ThreadNameUpdatedNotification,
        summary: Option<Box<Thread>>,
        update: ThreadSummaryUpdate,
    },
    ThreadClosed {
        payload: ThreadClosedNotification,
        summary: Option<Box<Thread>>,
        update: ThreadSummaryUpdate,
    },
    ThreadArchived {
        payload: ThreadArchivedNotification,
        summary: Option<Box<Thread>>,
        update: ThreadSummaryUpdate,
    },
    ThreadUnarchived {
        payload: ThreadUnarchivedNotification,
        previous_summary: Option<Box<Thread>>,
        summary: Option<Box<Thread>>,
        refresh: ThreadSummaryRefresh,
        update: ThreadSummaryUpdate,
    },
    OtherNotification(ServerNotification),
}

impl ThreadSummaryTracker {
    /// Applies one app-server event to the retained-summary cache and performs a
    /// follow-up `thread/read` refresh when the lifecycle edge is identity-only.
    pub async fn track_event(
        &mut self,
        request_handle: &AppServerRequestHandle,
        event: AppServerEvent,
    ) -> ThreadSummaryTrackedEvent {
        match event {
            AppServerEvent::Lagged { skipped } => ThreadSummaryTrackedEvent::Lagged { skipped },
            AppServerEvent::ServerRequest(request) => {
                let thread_id = server_request_thread_id(&request).map(ToOwned::to_owned);
                let summary = thread_id
                    .as_deref()
                    .and_then(|thread_id| self.get(thread_id))
                    .cloned()
                    .map(Box::new);
                ThreadSummaryTrackedEvent::ServerRequest {
                    request,
                    thread_id,
                    summary,
                }
            }
            AppServerEvent::Disconnected { message } => {
                ThreadSummaryTrackedEvent::Disconnected { message }
            }
            AppServerEvent::ServerNotification(notification) => match notification {
                ServerNotification::ThreadStarted(payload) => {
                    let _ = self
                        .apply_notification(&ServerNotification::ThreadStarted(payload.clone()));
                    ThreadSummaryTrackedEvent::ThreadStarted {
                        summary: Box::new(payload.thread.clone()),
                        payload,
                    }
                }
                ServerNotification::ThreadStatusChanged(payload) => {
                    let update = self.apply_notification(&ServerNotification::ThreadStatusChanged(
                        payload.clone(),
                    ));
                    let refresh =
                        if let ThreadSummaryUpdate::RequiresThreadRead { thread_id } = &update {
                            if payload.status == ThreadStatus::NotLoaded {
                                ThreadSummaryRefresh::NotNeeded
                            } else {
                                match self.refresh_thread(request_handle, thread_id.clone()).await {
                                    Ok(thread) => ThreadSummaryRefresh::Refreshed(Box::new(thread)),
                                    Err(error) => ThreadSummaryRefresh::Failed {
                                        thread_id: thread_id.clone(),
                                        error,
                                    },
                                }
                            }
                        } else {
                            ThreadSummaryRefresh::NotNeeded
                        };
                    ThreadSummaryTrackedEvent::ThreadStatusChanged {
                        summary: self.get(&payload.thread_id).cloned().map(Box::new),
                        payload,
                        refresh,
                        update,
                    }
                }
                ServerNotification::ThreadNameUpdated(payload) => {
                    let update = self.apply_notification(&ServerNotification::ThreadNameUpdated(
                        payload.clone(),
                    ));
                    ThreadSummaryTrackedEvent::ThreadNameUpdated {
                        summary: self.get(&payload.thread_id).cloned().map(Box::new),
                        payload,
                        update,
                    }
                }
                ServerNotification::ThreadClosed(payload) => {
                    let update =
                        self.apply_notification(&ServerNotification::ThreadClosed(payload.clone()));
                    ThreadSummaryTrackedEvent::ThreadClosed {
                        summary: self.get(&payload.thread_id).cloned().map(Box::new),
                        payload,
                        update,
                    }
                }
                ServerNotification::ThreadArchived(payload) => {
                    let update = self
                        .apply_notification(&ServerNotification::ThreadArchived(payload.clone()));
                    ThreadSummaryTrackedEvent::ThreadArchived {
                        summary: self.get(&payload.thread_id).cloned().map(Box::new),
                        payload,
                        update,
                    }
                }
                ServerNotification::ThreadUnarchived(payload) => {
                    let previous_summary = self.get(&payload.thread_id).cloned().map(Box::new);
                    let update = self
                        .apply_notification(&ServerNotification::ThreadUnarchived(payload.clone()));
                    let refresh =
                        if let ThreadSummaryUpdate::RequiresThreadRead { thread_id } = &update {
                            match self.refresh_thread(request_handle, thread_id.clone()).await {
                                Ok(thread) => ThreadSummaryRefresh::Refreshed(Box::new(thread)),
                                Err(error) => ThreadSummaryRefresh::Failed {
                                    thread_id: thread_id.clone(),
                                    error,
                                },
                            }
                        } else {
                            ThreadSummaryRefresh::NotNeeded
                        };
                    ThreadSummaryTrackedEvent::ThreadUnarchived {
                        summary: self.get(&payload.thread_id).cloned().map(Box::new),
                        payload,
                        previous_summary,
                        refresh,
                        update,
                    }
                }
                other => ThreadSummaryTrackedEvent::OtherNotification(other),
            },
        }
    }
}

fn server_request_thread_id(request: &ServerRequest) -> Option<&str> {
    match request {
        ServerRequest::CommandExecutionRequestApproval {
            params: CommandExecutionRequestApprovalParams { thread_id, .. },
            ..
        }
        | ServerRequest::FileChangeRequestApproval {
            params: FileChangeRequestApprovalParams { thread_id, .. },
            ..
        }
        | ServerRequest::McpServerElicitationRequest {
            params: McpServerElicitationRequestParams { thread_id, .. },
            ..
        }
        | ServerRequest::PermissionsRequestApproval {
            params: PermissionsRequestApprovalParams { thread_id, .. },
            ..
        } => Some(thread_id.as_str()),
        ServerRequest::ToolRequestUserInput { params, .. } => Some(params.thread_id.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadSummaryRefresh;
    use super::ThreadSummaryTrackedEvent;
    use crate::AppServerEvent;
    use crate::AppServerRequestHandle;
    use crate::RemoteAppServerClient;
    use crate::RemoteAppServerConnectArgs;
    use crate::ThreadSummaryTracker;
    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ServerRequest;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::ThreadStatusChangedNotification;
    use codex_app_server_protocol::ThreadUnarchivedNotification;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

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
            mode: codex_app_server_protocol::ThreadMode::ResidentAssistant,
            resident: true,
            path: None,
            cwd: PathBuf::from("/tmp/atlas"),
            cli_version: "1.0.0".to_string(),
            source: SessionSource::Cli,
            agent_nickname: None,
            agent_role: None,
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
            let notification: JSONRPCNotification =
                serde_json::from_str(&message).expect("initialized notification should decode");
            assert_eq!(notification.method, "initialized");

            let Message::Text(message) = websocket
                .next()
                .await
                .expect("thread/read request should arrive")
                .expect("websocket should stay open")
            else {
                panic!("expected thread/read request");
            };
            let request: JSONRPCRequest =
                serde_json::from_str(&message).expect("thread/read request should decode");
            assert_eq!(request.method, "thread/read");
            let response = JSONRPCMessage::Response(JSONRPCResponse {
                id: request.id,
                result: serde_json::to_value(ThreadReadResponse {
                    thread: make_thread("thread-1", Some("atlas"), ThreadStatus::NotLoaded),
                })
                .expect("thread/read response should encode"),
            });
            websocket
                .send(Message::Text(
                    serde_json::to_string(&response)
                        .expect("thread/read response should encode")
                        .into(),
                ))
                .await
                .expect("thread/read response should send");
        });
        format!("ws://{addr}")
    }

    async fn start_remote_server_that_disconnects_after_initialize() -> String {
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
            let notification: JSONRPCNotification =
                serde_json::from_str(&message).expect("initialized notification should decode");
            assert_eq!(notification.method, "initialized");
        });
        format!("ws://{addr}")
    }

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

    #[tokio::test]
    async fn track_event_refreshes_unknown_status_change() {
        let websocket_url = start_remote_server().await;
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();

        let event = tracker
            .track_event(
                &request_handle,
                AppServerEvent::ServerNotification(ServerNotification::ThreadStatusChanged(
                    ThreadStatusChangedNotification {
                        thread_id: "thread-1".to_string(),
                        status: ThreadStatus::SystemError,
                    },
                )),
            )
            .await;

        let ThreadSummaryTrackedEvent::ThreadStatusChanged {
            summary, refresh, ..
        } = event
        else {
            panic!("expected thread status changed event");
        };
        assert_eq!(
            summary.as_ref().map(|thread| thread.name.as_deref()),
            Some(Some("atlas"))
        );
        match refresh {
            ThreadSummaryRefresh::Refreshed(thread) => {
                assert_eq!(thread.id, "thread-1");
                assert_eq!(thread.name.as_deref(), Some("atlas"));
            }
            other => panic!("expected refreshed thread, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn track_event_retains_previous_summary_when_unarchive_refresh_fails() {
        let websocket_url = start_remote_server_that_disconnects_after_initialize().await;
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();
        tracker.upsert_thread(make_thread(
            "thread-1",
            Some("atlas"),
            ThreadStatus::NotLoaded,
        ));

        let event = tracker
            .track_event(
                &request_handle,
                AppServerEvent::ServerNotification(ServerNotification::ThreadUnarchived(
                    ThreadUnarchivedNotification {
                        thread_id: "thread-1".to_string(),
                    },
                )),
            )
            .await;

        let ThreadSummaryTrackedEvent::ThreadUnarchived {
            previous_summary,
            summary,
            refresh,
            ..
        } = event
        else {
            panic!("expected thread unarchived event");
        };
        assert_eq!(
            previous_summary
                .as_ref()
                .and_then(|thread| thread.name.as_deref()),
            Some("atlas")
        );
        assert_eq!(
            summary.as_ref().and_then(|thread| thread.name.as_deref()),
            Some("atlas")
        );
        match refresh {
            ThreadSummaryRefresh::Failed { thread_id, .. } => {
                assert_eq!(thread_id, "thread-1");
            }
            other => panic!("expected refresh failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn track_event_attaches_cached_summary_to_server_request() {
        let websocket_url = start_remote_server().await;
        let client = RemoteAppServerClient::connect(remote_connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();
        tracker.upsert_thread(make_thread("thread-1", Some("atlas"), ThreadStatus::Idle));

        let event = tracker
            .track_event(
                &request_handle,
                AppServerEvent::ServerRequest(ServerRequest::PermissionsRequestApproval {
                    request_id: codex_app_server_protocol::RequestId::String(
                        "srv-permissions".to_string(),
                    ),
                    params: codex_app_server_protocol::PermissionsRequestApprovalParams {
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        item_id: "item-1".to_string(),
                        reason: Some("need access".to_string()),
                        permissions: codex_app_server_protocol::RequestPermissionProfile {
                            network: Some(
                                codex_app_server_protocol::AdditionalNetworkPermissions {
                                    enabled: Some(true),
                                },
                            ),
                            file_system: None,
                        },
                    },
                }),
            )
            .await;

        let ThreadSummaryTrackedEvent::ServerRequest {
            request,
            thread_id,
            summary,
        } = event
        else {
            panic!("expected tracked server request event");
        };
        assert!(matches!(
            request,
            ServerRequest::PermissionsRequestApproval { .. }
        ));
        assert_eq!(thread_id.as_deref(), Some("thread-1"));
        let summary = summary.expect("server request should reuse cached summary");
        assert_eq!(summary.id, "thread-1");
        assert_eq!(summary.name.as_deref(), Some("atlas"));
        assert_eq!(
            summary.mode,
            codex_app_server_protocol::ThreadMode::ResidentAssistant
        );
    }
}
