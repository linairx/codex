use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedReadParams;
use codex_app_server_protocol::ThreadLoadedReadResponse;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStatus;

use crate::AppServerEvent;
use crate::AppServerRequestHandle;
use crate::TypedRequestError;

/// Tracks the last authoritative thread summaries observed by a client.
///
/// This helper is intended for bridge-style consumers that want to:
///
/// - bootstrap a thread overview from `thread/list` / `thread/loaded/read`
/// - retain the last authoritative `Thread` summary per thread id
/// - apply incremental lifecycle notifications on top of that retained summary
/// - explicitly refresh with `thread/read` when a notification is identity-only
///   or the client has not yet seen an authoritative summary for that thread id
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ThreadSummaryTracker {
    threads: BTreeMap<String, Thread>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Summary stats for a paginated bootstrap surface.
pub struct ThreadSummaryBootstrapStats {
    pub pages_fetched: usize,
    pub threads_observed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Aggregate result returned after bootstrapping both stored and loaded thread summaries.
pub struct ThreadSummaryBootstrapResult {
    pub thread_list: ThreadSummaryBootstrapStats,
    pub thread_loaded_read: ThreadSummaryBootstrapStats,
    pub thread_list_threads: usize,
    pub thread_loaded_read_threads: usize,
}

#[derive(Debug, Clone, PartialEq)]
/// Request params used to bootstrap retained summaries from both summary reads.
pub struct ThreadSummaryBootstrapParams {
    pub thread_list: ThreadListParams,
    pub thread_loaded_read: ThreadLoadedReadParams,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Outcome of applying a lifecycle increment onto the retained summary cache.
pub enum ThreadSummaryUpdate {
    Applied,
    Ignored,
    RequiresThreadRead { thread_id: String },
}

static THREAD_SUMMARY_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_thread_summary_request_id() -> RequestId {
    let id = THREAD_SUMMARY_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    RequestId::String(format!("thread-summary-{id}"))
}

impl ThreadSummaryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the last authoritative summary observed for `thread_id`, if any.
    pub fn get(&self, thread_id: &str) -> Option<&Thread> {
        self.threads.get(thread_id)
    }

    /// Iterates over retained thread summaries keyed by thread id.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Thread)> {
        self.threads.iter()
    }

    /// Inserts or replaces the authoritative summary for a thread.
    pub fn upsert_thread(&mut self, thread: Thread) {
        self.threads.insert(thread.id.clone(), thread);
    }

    /// Bootstraps the tracker from all pages of `thread/list` and `thread/loaded/read`.
    pub async fn bootstrap_all_pages(
        &mut self,
        request_handle: &AppServerRequestHandle,
        params: ThreadSummaryBootstrapParams,
    ) -> Result<ThreadSummaryBootstrapResult, TypedRequestError> {
        let thread_list = self
            .bootstrap_thread_list_all_pages(request_handle, params.thread_list)
            .await?;
        let thread_loaded_read = self
            .bootstrap_thread_loaded_read_all_pages(request_handle, params.thread_loaded_read)
            .await?;

        Ok(ThreadSummaryBootstrapResult {
            thread_list_threads: thread_list.threads_observed,
            thread_loaded_read_threads: thread_loaded_read.threads_observed,
            thread_list,
            thread_loaded_read,
        })
    }

    /// Fetches one `thread/list` page and retains every returned thread summary.
    pub async fn fetch_thread_list_page(
        &mut self,
        request_handle: &AppServerRequestHandle,
        params: ThreadListParams,
    ) -> Result<ThreadListResponse, TypedRequestError> {
        let response: ThreadListResponse = request_handle
            .request_typed(ClientRequest::ThreadList {
                request_id: next_thread_summary_request_id(),
                params,
            })
            .await?;
        for thread in &response.data {
            self.upsert_thread(thread.clone());
        }
        Ok(response)
    }

    /// Consumes every page from `thread/list`, retaining each returned `Thread`.
    pub async fn bootstrap_thread_list_all_pages(
        &mut self,
        request_handle: &AppServerRequestHandle,
        mut params: ThreadListParams,
    ) -> Result<ThreadSummaryBootstrapStats, TypedRequestError> {
        let mut pages_fetched = 0usize;
        let mut threads_observed = 0usize;

        loop {
            let response = self
                .fetch_thread_list_page(request_handle, params.clone())
                .await?;
            pages_fetched += 1;
            threads_observed += response.data.len();

            let Some(next_cursor) = response.next_cursor else {
                break;
            };
            params.cursor = Some(next_cursor);
        }

        Ok(ThreadSummaryBootstrapStats {
            pages_fetched,
            threads_observed,
        })
    }

    /// Fetches one `thread/loaded/read` page and retains every returned thread summary.
    pub async fn fetch_thread_loaded_read_page(
        &mut self,
        request_handle: &AppServerRequestHandle,
        params: ThreadLoadedReadParams,
    ) -> Result<ThreadLoadedReadResponse, TypedRequestError> {
        let response: ThreadLoadedReadResponse = request_handle
            .request_typed(ClientRequest::ThreadLoadedRead {
                request_id: next_thread_summary_request_id(),
                params,
            })
            .await?;
        for thread in &response.data {
            self.upsert_thread(thread.clone());
        }
        Ok(response)
    }

    /// Consumes every page from `thread/loaded/read`, retaining each returned `Thread`.
    pub async fn bootstrap_thread_loaded_read_all_pages(
        &mut self,
        request_handle: &AppServerRequestHandle,
        mut params: ThreadLoadedReadParams,
    ) -> Result<ThreadSummaryBootstrapStats, TypedRequestError> {
        let mut pages_fetched = 0usize;
        let mut threads_observed = 0usize;

        loop {
            let response = self
                .fetch_thread_loaded_read_page(request_handle, params.clone())
                .await?;
            pages_fetched += 1;
            threads_observed += response.data.len();

            let Some(next_cursor) = response.next_cursor else {
                break;
            };
            params.cursor = Some(next_cursor);
        }

        Ok(ThreadSummaryBootstrapStats {
            pages_fetched,
            threads_observed,
        })
    }

    /// Refreshes a single thread via `thread/read` and stores the returned summary.
    pub async fn refresh_thread(
        &mut self,
        request_handle: &AppServerRequestHandle,
        thread_id: impl Into<String>,
    ) -> Result<Thread, TypedRequestError> {
        let thread_id = thread_id.into();
        let response: ThreadReadResponse = request_handle
            .request_typed(ClientRequest::ThreadRead {
                request_id: next_thread_summary_request_id(),
                params: ThreadReadParams {
                    thread_id: thread_id.clone(),
                    include_turns: false,
                },
            })
            .await?;
        self.upsert_thread(response.thread.clone());
        Ok(response.thread)
    }

    /// Applies an incremental thread lifecycle notification onto the retained cache.
    pub fn apply_notification(&mut self, notification: &ServerNotification) -> ThreadSummaryUpdate {
        match notification {
            ServerNotification::ThreadStarted(payload) => {
                self.upsert_thread(payload.thread.clone());
                ThreadSummaryUpdate::Applied
            }
            ServerNotification::ThreadStatusChanged(payload) => {
                let Some(thread) = self.threads.get_mut(&payload.thread_id) else {
                    return if payload.status == ThreadStatus::NotLoaded {
                        ThreadSummaryUpdate::Ignored
                    } else {
                        ThreadSummaryUpdate::RequiresThreadRead {
                            thread_id: payload.thread_id.clone(),
                        }
                    };
                };
                thread.status = payload.status.clone();
                ThreadSummaryUpdate::Applied
            }
            ServerNotification::ThreadNameUpdated(payload) => {
                let Some(thread) = self.threads.get_mut(&payload.thread_id) else {
                    return ThreadSummaryUpdate::Ignored;
                };
                thread.name = payload.thread_name.clone();
                ThreadSummaryUpdate::Applied
            }
            ServerNotification::ThreadClosed(payload) => {
                let Some(thread) = self.threads.get_mut(&payload.thread_id) else {
                    return ThreadSummaryUpdate::Ignored;
                };
                thread.status = ThreadStatus::NotLoaded;
                ThreadSummaryUpdate::Applied
            }
            ServerNotification::ThreadArchived(payload) => {
                let Some(thread) = self.threads.get_mut(&payload.thread_id) else {
                    return ThreadSummaryUpdate::Ignored;
                };
                thread.status = ThreadStatus::NotLoaded;
                ThreadSummaryUpdate::Applied
            }
            ServerNotification::ThreadUnarchived(payload) => {
                ThreadSummaryUpdate::RequiresThreadRead {
                    thread_id: payload.thread_id.clone(),
                }
            }
            _ => ThreadSummaryUpdate::Ignored,
        }
    }

    /// Applies an `AppServerEvent`, ignoring non-notification events.
    pub fn apply_event(&mut self, event: &AppServerEvent) -> ThreadSummaryUpdate {
        match event {
            AppServerEvent::ServerNotification(notification) => {
                self.apply_notification(notification)
            }
            AppServerEvent::Lagged { .. }
            | AppServerEvent::ServerRequest(_)
            | AppServerEvent::Disconnected { .. } => ThreadSummaryUpdate::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codex_app_server_protocol::JSONRPCMessage;
    use codex_app_server_protocol::JSONRPCNotification;
    use codex_app_server_protocol::JSONRPCRequest;
    use codex_app_server_protocol::JSONRPCResponse;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::SessionSource as ApiSessionSource;
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadArchivedNotification;
    use codex_app_server_protocol::ThreadClosedNotification;
    use codex_app_server_protocol::ThreadMode;
    use codex_app_server_protocol::ThreadNameUpdatedNotification;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_app_server_protocol::ThreadStatusChangedNotification;
    use codex_app_server_protocol::ThreadUnarchivedNotification;
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::RemoteAppServerClient;
    use crate::RemoteAppServerConnectArgs;

    fn make_thread(thread_id: &str, status: ThreadStatus) -> Thread {
        Thread {
            id: thread_id.to_string(),
            forked_from_id: None,
            preview: "resident thread".to_string(),
            ephemeral: false,
            model_provider: "mock_provider".to_string(),
            created_at: 1_736_078_400,
            updated_at: 1_736_078_400,
            status,
            mode: ThreadMode::ResidentAssistant,
            resident: true,
            path: Some(PathBuf::from(format!("/tmp/{thread_id}.jsonl"))),
            cwd: PathBuf::from("/workspace"),
            cli_version: "0.0.0-test".to_string(),
            source: ApiSessionSource::Cli,
            agent_nickname: None,
            agent_role: None,
            git_info: None,
            name: Some("Atlas".to_string()),
            turns: Vec::new(),
        }
    }

    async fn start_remote_server<F, Fut>(handler: F) -> String
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept should succeed");
            let websocket = accept_async(stream).await.expect("accept should succeed");
            handler(websocket).await;
        });
        format!("ws://{addr}")
    }

    async fn expect_initialize(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
        let Message::Text(message) = websocket
            .next()
            .await
            .expect("initialize request should arrive")
            .expect("websocket should stay open")
        else {
            panic!("expected initialize text message");
        };
        let request: JSONRPCRequest =
            serde_json::from_str(&message).expect("initialize request should decode");
        assert_eq!(request.method, "initialize");
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
            panic!("expected initialized text message");
        };
        let notification: JSONRPCNotification =
            serde_json::from_str(&message).expect("initialized notification should decode");
        assert_eq!(notification.method, "initialized");
    }

    async fn respond_to_request(
        websocket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        expected_method: &str,
        result: serde_json::Value,
    ) {
        let Message::Text(message) = websocket
            .next()
            .await
            .expect("request should arrive")
            .expect("websocket should stay open")
        else {
            panic!("expected request text message");
        };
        let request: JSONRPCRequest =
            serde_json::from_str(&message).expect("request should decode");
        assert_eq!(request.method, expected_method);
        let response = JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result,
        });
        websocket
            .send(Message::Text(
                serde_json::to_string(&response)
                    .expect("response should encode")
                    .into(),
            ))
            .await
            .expect("response should send");
    }

    fn connect_args(websocket_url: String) -> RemoteAppServerConnectArgs {
        RemoteAppServerConnectArgs {
            websocket_url,
            auth_token: None,
            client_name: "tracker-test".to_string(),
            client_version: "0.0.0-test".to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: 8,
        }
    }

    #[tokio::test]
    async fn bootstrap_all_pages_reads_thread_list_and_loaded_read() {
        let websocket_url = start_remote_server(|mut websocket| async move {
            expect_initialize(&mut websocket).await;
            respond_to_request(
                &mut websocket,
                "thread/list",
                serde_json::to_value(ThreadListResponse {
                    data: vec![make_thread("thread-list-1", ThreadStatus::NotLoaded)],
                    next_cursor: Some("cursor-2".to_string()),
                })
                .expect("thread/list response should encode"),
            )
            .await;
            respond_to_request(
                &mut websocket,
                "thread/list",
                serde_json::to_value(ThreadListResponse {
                    data: vec![make_thread("thread-list-2", ThreadStatus::NotLoaded)],
                    next_cursor: None,
                })
                .expect("thread/list response should encode"),
            )
            .await;
            respond_to_request(
                &mut websocket,
                "thread/loaded/read",
                serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![make_thread(
                        "thread-loaded-1",
                        ThreadStatus::Active {
                            active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
                        },
                    )],
                    next_cursor: Some("cursor-3".to_string()),
                })
                .expect("thread/loaded/read response should encode"),
            )
            .await;
            respond_to_request(
                &mut websocket,
                "thread/loaded/read",
                serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![make_thread("thread-loaded-2", ThreadStatus::Idle)],
                    next_cursor: None,
                })
                .expect("thread/loaded/read response should encode"),
            )
            .await;
        })
        .await;

        let client = RemoteAppServerClient::connect(connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = crate::AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();

        let bootstrap = tracker
            .bootstrap_all_pages(
                &request_handle,
                ThreadSummaryBootstrapParams {
                    thread_list: ThreadListParams {
                        cursor: None,
                        limit: Some(1),
                        sort_key: None,
                        model_providers: None,
                        source_kinds: None,
                        archived: None,
                        cwd: None,
                        search_term: None,
                    },
                    thread_loaded_read: ThreadLoadedReadParams {
                        cursor: None,
                        limit: Some(1),
                        model_providers: None,
                        source_kinds: None,
                        cwd: None,
                    },
                },
            )
            .await
            .expect("bootstrap should succeed");

        assert_eq!(
            bootstrap,
            ThreadSummaryBootstrapResult {
                thread_list: ThreadSummaryBootstrapStats {
                    pages_fetched: 2,
                    threads_observed: 2,
                },
                thread_loaded_read: ThreadSummaryBootstrapStats {
                    pages_fetched: 2,
                    threads_observed: 2,
                },
                thread_list_threads: 2,
                thread_loaded_read_threads: 2,
            }
        );
        assert_eq!(tracker.threads.len(), 4);
        assert_eq!(
            tracker.get("thread-loaded-1").map(|thread| &thread.status),
            Some(&ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
            })
        );
    }

    #[tokio::test]
    async fn fetch_thread_list_page_retains_returned_threads() {
        let websocket_url = start_remote_server(|mut websocket| async move {
            expect_initialize(&mut websocket).await;
            respond_to_request(
                &mut websocket,
                "thread/list",
                serde_json::to_value(ThreadListResponse {
                    data: vec![
                        make_thread("thread-list-1", ThreadStatus::NotLoaded),
                        make_thread("thread-list-2", ThreadStatus::Idle),
                    ],
                    next_cursor: Some("cursor-2".to_string()),
                })
                .expect("thread/list response should encode"),
            )
            .await;
        })
        .await;

        let client = RemoteAppServerClient::connect(connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = crate::AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();

        let response = tracker
            .fetch_thread_list_page(
                &request_handle,
                ThreadListParams {
                    cursor: None,
                    limit: Some(2),
                    sort_key: None,
                    model_providers: None,
                    source_kinds: None,
                    archived: None,
                    cwd: None,
                    search_term: None,
                },
            )
            .await
            .expect("thread/list page should succeed");

        assert_eq!(response.next_cursor.as_deref(), Some("cursor-2"));
        assert_eq!(response.data.len(), 2);
        assert_eq!(tracker.threads.len(), 2);
        assert_eq!(
            tracker.get("thread-list-2").map(|thread| &thread.status),
            Some(&ThreadStatus::Idle)
        );
    }

    #[tokio::test]
    async fn fetch_thread_loaded_read_page_retains_returned_threads() {
        let websocket_url = start_remote_server(|mut websocket| async move {
            expect_initialize(&mut websocket).await;
            respond_to_request(
                &mut websocket,
                "thread/loaded/read",
                serde_json::to_value(ThreadLoadedReadResponse {
                    data: vec![make_thread(
                        "thread-loaded-1",
                        ThreadStatus::Active {
                            active_flags: vec![ThreadActiveFlag::BackgroundTerminalRunning],
                        },
                    )],
                    next_cursor: Some("cursor-3".to_string()),
                })
                .expect("thread/loaded/read response should encode"),
            )
            .await;
        })
        .await;

        let client = RemoteAppServerClient::connect(connect_args(websocket_url))
            .await
            .expect("remote client should connect");
        let request_handle = crate::AppServerRequestHandle::Remote(client.request_handle());
        let mut tracker = ThreadSummaryTracker::new();

        let response = tracker
            .fetch_thread_loaded_read_page(
                &request_handle,
                ThreadLoadedReadParams {
                    cursor: None,
                    limit: Some(1),
                    model_providers: None,
                    source_kinds: None,
                    cwd: None,
                },
            )
            .await
            .expect("thread/loaded/read page should succeed");

        assert_eq!(response.next_cursor.as_deref(), Some("cursor-3"));
        assert_eq!(response.data.len(), 1);
        assert_eq!(tracker.threads.len(), 1);
        assert_eq!(
            tracker.get("thread-loaded-1").map(|thread| &thread.status),
            Some(&ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::BackgroundTerminalRunning],
            })
        );
    }

    #[test]
    fn apply_notification_retains_authoritative_summary_for_lifecycle_edges() {
        let mut tracker = ThreadSummaryTracker::new();
        tracker.upsert_thread(make_thread("thread-1", ThreadStatus::Idle));

        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadStatusChanged(
                ThreadStatusChangedNotification {
                    thread_id: "thread-1".to_string(),
                    status: ThreadStatus::SystemError,
                }
            )),
            ThreadSummaryUpdate::Applied
        );
        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadNameUpdated(
                ThreadNameUpdatedNotification {
                    thread_id: "thread-1".to_string(),
                    thread_name: Some("Atlas repaired".to_string()),
                }
            )),
            ThreadSummaryUpdate::Applied
        );
        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadClosed(
                ThreadClosedNotification {
                    thread_id: "thread-1".to_string(),
                }
            )),
            ThreadSummaryUpdate::Applied
        );

        assert_eq!(
            tracker.get("thread-1"),
            Some(&Thread {
                status: ThreadStatus::NotLoaded,
                name: Some("Atlas repaired".to_string()),
                ..make_thread("thread-1", ThreadStatus::Idle)
            })
        );
    }

    #[test]
    fn apply_notification_requests_refresh_when_authoritative_summary_is_missing() {
        let mut tracker = ThreadSummaryTracker::new();

        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadStatusChanged(
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::SystemError,
                }
            )),
            ThreadSummaryUpdate::RequiresThreadRead {
                thread_id: "thread-unknown".to_string(),
            }
        );
        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadStatusChanged(
                ThreadStatusChangedNotification {
                    thread_id: "thread-unknown".to_string(),
                    status: ThreadStatus::NotLoaded,
                }
            )),
            ThreadSummaryUpdate::Ignored
        );
        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadUnarchived(
                ThreadUnarchivedNotification {
                    thread_id: "thread-unknown".to_string(),
                }
            )),
            ThreadSummaryUpdate::RequiresThreadRead {
                thread_id: "thread-unknown".to_string(),
            }
        );
        assert_eq!(
            tracker.apply_notification(&ServerNotification::ThreadArchived(
                ThreadArchivedNotification {
                    thread_id: "thread-unknown".to_string(),
                }
            )),
            ThreadSummaryUpdate::Ignored
        );
    }

    #[test]
    fn apply_notification_uses_started_snapshot_as_authoritative_summary() {
        let mut tracker = ThreadSummaryTracker::new();
        let thread = make_thread("thread-started", ThreadStatus::Idle);

        assert_eq!(
            tracker.apply_event(&crate::AppServerEvent::ServerNotification(
                ServerNotification::ThreadStarted(ThreadStartedNotification {
                    thread: thread.clone(),
                })
            )),
            ThreadSummaryUpdate::Applied
        );
        assert_eq!(tracker.get("thread-started"), Some(&thread));
    }

    #[test]
    fn apply_event_ignores_non_notification_events() {
        let mut tracker = ThreadSummaryTracker::new();

        assert_eq!(
            tracker.apply_event(&crate::AppServerEvent::Lagged { skipped: 1 }),
            ThreadSummaryUpdate::Ignored
        );
        assert_eq!(
            tracker.apply_event(&crate::AppServerEvent::Disconnected {
                message: "socket closed".to_string(),
            }),
            ThreadSummaryUpdate::Ignored
        );
    }
}
