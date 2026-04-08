use std::collections::HashMap;

use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownThread {
    pub thread_id: String,
    pub thread_mode: ThreadMode,
}

#[derive(Debug, Default)]
pub struct State {
    pub pending: HashMap<RequestId, PendingRequest>,
    pub thread_id: Option<String>,
    pub known_threads: Vec<KnownThread>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRequest {
    Start,
    Resume,
    List,
    LoadedList,
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
}
