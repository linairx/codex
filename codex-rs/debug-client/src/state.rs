use std::collections::HashMap;

use codex_app_server_client::ThreadSummaryTracker;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadMode;
use codex_app_server_protocol::ThreadStatus;

#[derive(Debug, Clone, PartialEq)]
pub struct KnownThread {
    pub thread_id: String,
    pub thread_name: Option<String>,
    pub thread_mode: ThreadMode,
    pub thread_status: ThreadStatus,
}

#[derive(Debug, Default)]
pub struct State {
    pub pending: HashMap<RequestId, PendingRequest>,
    pub thread_id: Option<String>,
    pub thread_summaries: ThreadSummaryTracker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingRequest {
    Start,
    Resume,
    Read { thread_id: String },
    List,
    LoadedList,
    LoadedRead,
}

#[derive(Debug, Clone)]
pub enum ReaderEvent {
    ThreadReady {
        thread_id: String,
        thread_mode: ThreadMode,
    },
    ThreadList {
        threads: Vec<KnownThread>,
        next_cursor: Option<String>,
    },
    LoadedThreadList {
        thread_ids: Vec<String>,
        next_cursor: Option<String>,
    },
    LoadedThreadRead {
        threads: Vec<KnownThread>,
        next_cursor: Option<String>,
    },
}
