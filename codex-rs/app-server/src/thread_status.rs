#[cfg(test)]
use crate::outgoing_message::OutgoingEnvelope;
#[cfg(test)]
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_core::file_watcher::FileWatcher;
use codex_core::file_watcher::FileWatcherSubscriber;
use codex_core::file_watcher::ThrottledWatchReceiver;
use codex_core::file_watcher::WatchPath;
use codex_core::file_watcher::WatchRegistration;
use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tracing::warn;

const WORKSPACE_CHANGED_NOTIFICATION_DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub(crate) struct ThreadWatchManager {
    state: Arc<Mutex<ThreadWatchState>>,
    outgoing: Option<Arc<OutgoingMessageSender>>,
    running_turn_count_tx: watch::Sender<usize>,
    file_watcher: Arc<FileWatcher>,
    workspace_watch_state: Arc<Mutex<HashMap<String, WorkspaceWatchEntry>>>,
}

pub(crate) struct ThreadWatchActiveGuard {
    manager: ThreadWatchManager,
    thread_id: String,
    guard_type: ThreadWatchActiveGuardType,
    handle: tokio::runtime::Handle,
}

impl ThreadWatchActiveGuard {
    fn new(
        manager: ThreadWatchManager,
        thread_id: String,
        guard_type: ThreadWatchActiveGuardType,
    ) -> Self {
        Self {
            manager,
            thread_id,
            guard_type,
            handle: tokio::runtime::Handle::current(),
        }
    }
}

impl Drop for ThreadWatchActiveGuard {
    fn drop(&mut self) {
        let manager = self.manager.clone();
        let thread_id = self.thread_id.clone();
        let guard_type = self.guard_type;
        self.handle.spawn(async move {
            manager
                .note_active_guard_released(thread_id, guard_type)
                .await;
        });
    }
}

#[derive(Clone, Copy)]
enum ThreadWatchActiveGuardType {
    Permission,
    UserInput,
}

struct WorkspaceWatchEntry {
    cwd: std::path::PathBuf,
    terminate_tx: oneshot::Sender<oneshot::Sender<()>>,
    _subscriber: FileWatcherSubscriber,
    _registration: WatchRegistration,
}

impl Default for ThreadWatchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadWatchManager {
    pub(crate) fn new() -> Self {
        let file_watcher = match FileWatcher::new() {
            Ok(file_watcher) => Arc::new(file_watcher),
            Err(err) => {
                warn!("thread watch manager falling back to noop core watcher: {err}");
                Arc::new(FileWatcher::noop())
            }
        };
        Self::new_with_file_watcher(None, file_watcher)
    }

    fn new_with_file_watcher(
        outgoing: Option<Arc<OutgoingMessageSender>>,
        file_watcher: Arc<FileWatcher>,
    ) -> Self {
        let (running_turn_count_tx, _running_turn_count_rx) = watch::channel(0);
        Self {
            state: Arc::new(Mutex::new(ThreadWatchState::default())),
            outgoing,
            running_turn_count_tx,
            file_watcher,
            workspace_watch_state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn new_with_outgoing(outgoing: Arc<OutgoingMessageSender>) -> Self {
        let file_watcher = match FileWatcher::new() {
            Ok(file_watcher) => Arc::new(file_watcher),
            Err(err) => {
                warn!("thread watch manager falling back to noop core watcher: {err}");
                Arc::new(FileWatcher::noop())
            }
        };
        Self::new_with_file_watcher(Some(outgoing), file_watcher)
    }

    pub(crate) async fn upsert_thread(&self, thread: Thread) {
        self.ensure_workspace_watch(&thread).await;
        self.mutate_and_publish(move |state| {
            state.upsert_thread(thread.id, /*emit_notification*/ true)
        })
        .await;
    }

    pub(crate) async fn upsert_thread_silently(&self, thread: Thread) {
        self.ensure_workspace_watch(&thread).await;
        self.mutate_and_publish(move |state| {
            state.upsert_thread(thread.id, /*emit_notification*/ false)
        })
        .await;
    }

    pub(crate) async fn remove_thread(&self, thread_id: &str) {
        self.remove_workspace_watch(thread_id).await;
        let thread_id = thread_id.to_string();
        self.mutate_and_publish(move |state| state.remove_thread(&thread_id))
            .await;
    }

    pub(crate) async fn loaded_status_for_thread(&self, thread_id: &str) -> ThreadStatus {
        self.state.lock().await.loaded_status_for_thread(thread_id)
    }

    pub(crate) async fn loaded_statuses_for_threads(
        &self,
        thread_ids: Vec<String>,
    ) -> HashMap<String, ThreadStatus> {
        let state = self.state.lock().await;
        thread_ids
            .into_iter()
            .map(|thread_id| {
                let status = state.loaded_status_for_thread(&thread_id);
                (thread_id, status)
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) async fn running_turn_count(&self) -> usize {
        self.state
            .lock()
            .await
            .runtime_by_thread_id
            .values()
            .filter(|runtime| runtime.running)
            .count()
    }

    pub(crate) fn subscribe_running_turn_count(&self) -> watch::Receiver<usize> {
        self.running_turn_count_tx.subscribe()
    }

    pub(crate) async fn note_turn_started(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.is_loaded = true;
            runtime.running = true;
            runtime.has_system_error = false;
            runtime.workspace_changed = false;
        })
        .await;
    }

    pub(crate) async fn note_turn_completed(&self, thread_id: &str, _failed: bool) {
        self.clear_active_state(thread_id).await;
    }

    pub(crate) async fn note_turn_interrupted(&self, thread_id: &str) {
        self.clear_active_state(thread_id).await;
    }

    pub(crate) async fn note_thread_shutdown(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.running = false;
            runtime.running_background_terminals = 0;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
            runtime.is_loaded = false;
        })
        .await;
    }

    pub(crate) async fn note_system_error(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.running = false;
            runtime.running_background_terminals = 0;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
            runtime.has_system_error = true;
        })
        .await;
    }

    async fn clear_active_state(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, move |runtime| {
            runtime.running = false;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
        })
        .await;
    }

    pub(crate) async fn note_background_terminal_started(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, move |runtime| {
            runtime.is_loaded = true;
            runtime.running_background_terminals =
                runtime.running_background_terminals.saturating_add(1);
        })
        .await;
    }

    pub(crate) async fn note_background_terminal_ended(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, move |runtime| {
            runtime.running_background_terminals =
                runtime.running_background_terminals.saturating_sub(1);
        })
        .await;
    }

    pub(crate) async fn note_permission_requested(
        &self,
        thread_id: &str,
    ) -> ThreadWatchActiveGuard {
        self.note_pending_request(thread_id, ThreadWatchActiveGuardType::Permission)
            .await
    }

    pub(crate) async fn note_user_input_requested(
        &self,
        thread_id: &str,
    ) -> ThreadWatchActiveGuard {
        self.note_pending_request(thread_id, ThreadWatchActiveGuardType::UserInput)
            .await
    }

    async fn note_pending_request(
        &self,
        thread_id: &str,
        guard_type: ThreadWatchActiveGuardType,
    ) -> ThreadWatchActiveGuard {
        self.update_runtime_for_thread(thread_id, move |runtime| {
            runtime.is_loaded = true;
            let counter = Self::pending_counter(runtime, guard_type);
            *counter = counter.saturating_add(1);
        })
        .await;
        ThreadWatchActiveGuard::new(self.clone(), thread_id.to_string(), guard_type)
    }

    async fn mutate_and_publish<F>(&self, mutate: F)
    where
        F: FnOnce(&mut ThreadWatchState) -> Option<ThreadStatusChangedNotification>,
    {
        let (notification, running_turn_count) = {
            let mut state = self.state.lock().await;
            let notification = mutate(&mut state);
            let running_turn_count = state
                .runtime_by_thread_id
                .values()
                .filter(|runtime| runtime.running)
                .count();
            (notification, running_turn_count)
        };
        let _ = self.running_turn_count_tx.send(running_turn_count);

        if let Some(notification) = notification
            && let Some(outgoing) = &self.outgoing
        {
            outgoing
                .send_server_notification(ServerNotification::ThreadStatusChanged(notification))
                .await;
        }
    }

    async fn note_active_guard_released(
        &self,
        thread_id: String,
        guard_type: ThreadWatchActiveGuardType,
    ) {
        self.update_runtime_for_thread(&thread_id, move |runtime| {
            let counter = Self::pending_counter(runtime, guard_type);
            *counter = counter.saturating_sub(1);
        })
        .await;
    }

    async fn update_runtime_for_thread<F>(&self, thread_id: &str, update: F)
    where
        F: FnOnce(&mut RuntimeFacts),
    {
        let thread_id = thread_id.to_string();
        self.mutate_and_publish(move |state| state.update_runtime(&thread_id, update))
            .await;
    }

    async fn ensure_workspace_watch(&self, thread: &Thread) {
        let thread_id = thread.id.clone();
        if !thread.resident {
            self.remove_workspace_watch(&thread_id).await;
            return;
        }

        let cwd = thread.cwd.clone();
        let mut ignored_roots = Vec::new();
        let mut ignored_codex_home_files = Vec::new();
        let mut ignored_codex_home_parent = None;
        if let Some(sessions_root) = thread.path.as_ref().and_then(|path| {
            path.ancestors()
                .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "sessions"))
                .map(std::path::Path::to_path_buf)
        }) {
            ignored_roots.push(sessions_root.clone());
            if let Some(codex_home) = sessions_root.parent() {
                ignored_codex_home_parent = Some(codex_home.to_path_buf());
                ignored_roots.push(codex_home.join("tmp"));
                ignored_roots.push(codex_home.join("shell_snapshots"));
                ignored_codex_home_files.push("session_index.jsonl".to_string());
                ignored_codex_home_files.push("state".to_string());
                ignored_codex_home_files.push("logs".to_string());
            }
        }
        let mut watch_state = self.workspace_watch_state.lock().await;
        if watch_state
            .get(&thread_id)
            .is_some_and(|entry| entry.cwd == cwd)
        {
            return;
        }

        let existing = watch_state.remove(&thread_id);
        drop(watch_state);
        if let Some(existing) = existing {
            let (done_tx, done_rx) = oneshot::channel();
            let _ = existing.terminate_tx.send(done_tx);
            let _ = done_rx.await;
        }

        let (subscriber, rx) = self.file_watcher.add_subscriber();
        let registration = subscriber.register_paths(vec![WatchPath {
            path: cwd.clone(),
            recursive: true,
        }]);
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let watched_cwd = cwd.clone();
        let mut watch_state = self.workspace_watch_state.lock().await;
        watch_state.insert(
            thread_id.clone(),
            WorkspaceWatchEntry {
                cwd,
                terminate_tx,
                _subscriber: subscriber,
                _registration: registration,
            },
        );
        drop(watch_state);

        let manager = self.clone();
        tokio::spawn(async move {
            let mut rx = ThrottledWatchReceiver::new(rx, WORKSPACE_CHANGED_NOTIFICATION_DEBOUNCE);
            tokio::pin!(terminate_rx);
            loop {
                tokio::select! {
                    biased;
                    done_tx = &mut terminate_rx => {
                        if let Ok(done_tx) = done_tx {
                            let _ = done_tx.send(());
                        }
                        break;
                    }
                    event = rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        if event.paths.iter().all(|path| {
                            path == &watched_cwd
                                || ignored_roots.iter().any(|root| path.starts_with(root))
                                || ignored_codex_home_parent.as_ref().is_some_and(|codex_home| {
                                    path.parent() == Some(codex_home.as_path())
                                        && path
                                            .file_name()
                                            .and_then(std::ffi::OsStr::to_str)
                                            .is_some_and(|name| {
                                                name == "session_index.jsonl"
                                                    || ignored_codex_home_files.iter().any(|prefix| {
                                                        prefix != "session_index.jsonl"
                                                            && name.starts_with(prefix)
                                                            && name.contains(".sqlite")
                                                    })
                                            })
                                })
                        })
                        {
                            continue;
                        }
                        manager.note_workspace_changed(&thread_id).await;
                    }
                }
            }
        });
    }

    async fn remove_workspace_watch(&self, thread_id: &str) {
        let entry = {
            let mut workspace_watch_state = self.workspace_watch_state.lock().await;
            workspace_watch_state.remove(thread_id)
        };
        if let Some(entry) = entry {
            let (done_tx, done_rx) = oneshot::channel();
            let _ = entry.terminate_tx.send(done_tx);
            let _ = done_rx.await;
        }
    }

    async fn note_workspace_changed(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.workspace_changed = true;
        })
        .await;
    }

    fn pending_counter(
        runtime: &mut RuntimeFacts,
        guard_type: ThreadWatchActiveGuardType,
    ) -> &mut u32 {
        match guard_type {
            ThreadWatchActiveGuardType::Permission => &mut runtime.pending_permission_requests,
            ThreadWatchActiveGuardType::UserInput => &mut runtime.pending_user_input_requests,
        }
    }
}

pub(crate) fn resolve_thread_status(
    status: ThreadStatus,
    has_in_progress_turn: bool,
) -> ThreadStatus {
    // Running-turn events can arrive before the watch runtime state is observed by
    // the listener loop. In that window we prefer to reflect a real active turn as
    // `Active` instead of `Idle`/`NotLoaded`.
    if has_in_progress_turn && matches!(status, ThreadStatus::Idle | ThreadStatus::NotLoaded) {
        return ThreadStatus::Active {
            active_flags: Vec::new(),
        };
    }

    status
}

#[derive(Default)]
struct ThreadWatchState {
    runtime_by_thread_id: HashMap<String, RuntimeFacts>,
}

impl ThreadWatchState {
    fn upsert_thread(
        &mut self,
        thread_id: String,
        emit_notification: bool,
    ) -> Option<ThreadStatusChangedNotification> {
        let previous_status = self.status_for(&thread_id);
        let runtime = self
            .runtime_by_thread_id
            .entry(thread_id.clone())
            .or_default();
        runtime.is_loaded = true;
        if emit_notification {
            self.status_changed_notification(thread_id, previous_status)
        } else {
            None
        }
    }

    fn remove_thread(&mut self, thread_id: &str) -> Option<ThreadStatusChangedNotification> {
        let previous_status = self.status_for(thread_id);
        self.runtime_by_thread_id.remove(thread_id);
        if previous_status.is_some() && previous_status != Some(ThreadStatus::NotLoaded) {
            Some(ThreadStatusChangedNotification {
                thread_id: thread_id.to_string(),
                status: ThreadStatus::NotLoaded,
            })
        } else {
            None
        }
    }

    fn update_runtime<F>(
        &mut self,
        thread_id: &str,
        mutate: F,
    ) -> Option<ThreadStatusChangedNotification>
    where
        F: FnOnce(&mut RuntimeFacts),
    {
        let previous_status = self.status_for(thread_id);
        let runtime = self
            .runtime_by_thread_id
            .entry(thread_id.to_string())
            .or_default();
        runtime.is_loaded = true;
        mutate(runtime);
        self.status_changed_notification(thread_id.to_string(), previous_status)
    }

    fn status_for(&self, thread_id: &str) -> Option<ThreadStatus> {
        self.runtime_by_thread_id
            .get(thread_id)
            .map(loaded_thread_status)
    }

    fn loaded_status_for_thread(&self, thread_id: &str) -> ThreadStatus {
        self.status_for(thread_id)
            .unwrap_or(ThreadStatus::NotLoaded)
    }

    fn status_changed_notification(
        &self,
        thread_id: String,
        previous_status: Option<ThreadStatus>,
    ) -> Option<ThreadStatusChangedNotification> {
        let status = self.status_for(&thread_id)?;

        if previous_status.as_ref() == Some(&status) {
            return None;
        }

        Some(ThreadStatusChangedNotification { thread_id, status })
    }
}

#[derive(Clone, Default)]
struct RuntimeFacts {
    is_loaded: bool,
    running: bool,
    running_background_terminals: u32,
    pending_permission_requests: u32,
    pending_user_input_requests: u32,
    has_system_error: bool,
    workspace_changed: bool,
}

fn loaded_thread_status(runtime: &RuntimeFacts) -> ThreadStatus {
    if !runtime.is_loaded {
        return ThreadStatus::NotLoaded;
    }

    let mut active_flags = Vec::new();
    if runtime.pending_permission_requests > 0 {
        active_flags.push(ThreadActiveFlag::WaitingOnApproval);
    }
    if runtime.pending_user_input_requests > 0 {
        active_flags.push(ThreadActiveFlag::WaitingOnUserInput);
    }
    if runtime.running_background_terminals > 0 {
        active_flags.push(ThreadActiveFlag::BackgroundTerminalRunning);
    }
    if runtime.workspace_changed {
        active_flags.push(ThreadActiveFlag::WorkspaceChanged);
    }

    if runtime.running || !active_flags.is_empty() {
        return ThreadStatus::Active { active_flags };
    }

    if runtime.has_system_error {
        return ThreadStatus::SystemError;
    }

    ThreadStatus::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tokio::time::Duration;
    use tokio::time::timeout;

    const INTERACTIVE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000001";
    const NON_INTERACTIVE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000002";

    #[tokio::test]
    async fn loaded_status_defaults_to_not_loaded_for_untracked_threads() {
        let manager = test_manager();

        assert_eq!(
            manager
                .loaded_status_for_thread("00000000-0000-0000-0000-000000000003")
                .await,
            ThreadStatus::NotLoaded,
        );
    }

    #[tokio::test]
    async fn tracks_non_interactive_thread_status() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                NON_INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::AppServer,
            ))
            .await;

        manager.note_turn_started(NON_INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(NON_INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![],
            },
        );
    }

    #[tokio::test]
    async fn status_updates_track_single_thread() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![],
            },
        );

        let permission_guard = manager
            .note_permission_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WaitingOnApproval],
            },
        );

        let user_input_guard = manager
            .note_user_input_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![
                    ThreadActiveFlag::WaitingOnApproval,
                    ThreadActiveFlag::WaitingOnUserInput,
                ],
            },
        );

        drop(permission_guard);
        wait_for_status(
            &manager,
            INTERACTIVE_THREAD_ID,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WaitingOnUserInput],
            },
        )
        .await;

        drop(user_input_guard);
        wait_for_status(
            &manager,
            INTERACTIVE_THREAD_ID,
            ThreadStatus::Active {
                active_flags: vec![],
            },
        )
        .await;

        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Idle,
        );
    }

    #[test]
    fn resolves_in_progress_turn_to_active_status() {
        let status = resolve_thread_status(ThreadStatus::Idle, /*has_in_progress_turn*/ true);
        assert_eq!(
            status,
            ThreadStatus::Active {
                active_flags: Vec::new(),
            }
        );

        let status =
            resolve_thread_status(ThreadStatus::NotLoaded, /*has_in_progress_turn*/ true);
        assert_eq!(
            status,
            ThreadStatus::Active {
                active_flags: Vec::new(),
            }
        );
    }

    #[test]
    fn keeps_status_when_no_in_progress_turn() {
        assert_eq!(
            resolve_thread_status(ThreadStatus::Idle, /*has_in_progress_turn*/ false),
            ThreadStatus::Idle
        );
        assert_eq!(
            resolve_thread_status(
                ThreadStatus::SystemError,
                /*has_in_progress_turn*/ false
            ),
            ThreadStatus::SystemError
        );
    }

    #[tokio::test]
    async fn system_error_sets_idle_flag_until_next_turn() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager.note_system_error(INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::SystemError,
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![],
            },
        );
    }

    #[tokio::test]
    async fn shutdown_marks_thread_not_loaded() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager.note_thread_shutdown(INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::NotLoaded,
        );
    }

    #[tokio::test]
    async fn loaded_statuses_default_to_not_loaded_for_untracked_threads() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;
        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;

        let statuses = manager
            .loaded_statuses_for_threads(vec![
                INTERACTIVE_THREAD_ID.to_string(),
                NON_INTERACTIVE_THREAD_ID.to_string(),
            ])
            .await;

        assert_eq!(
            statuses.get(INTERACTIVE_THREAD_ID),
            Some(&ThreadStatus::Active {
                active_flags: vec![],
            }),
        );
        assert_eq!(
            statuses.get(NON_INTERACTIVE_THREAD_ID),
            Some(&ThreadStatus::NotLoaded),
        );
    }

    #[tokio::test]
    async fn has_running_turns_tracks_runtime_running_flag_only() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        assert_eq!(manager.running_turn_count().await, 0);

        let _permission_guard = manager
            .note_permission_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(manager.running_turn_count().await, 0);

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(manager.running_turn_count().await, 1);

        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;
        assert_eq!(manager.running_turn_count().await, 0);
    }

    #[tokio::test]
    async fn background_terminals_keep_thread_active_after_turn_completion() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager
            .note_background_terminal_started(INTERACTIVE_THREAD_ID)
            .await;
        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::BackgroundTerminalRunning],
            },
        );

        manager
            .note_background_terminal_ended(INTERACTIVE_THREAD_ID)
            .await;

        wait_for_status(&manager, INTERACTIVE_THREAD_ID, ThreadStatus::Idle).await;
    }

    #[tokio::test]
    async fn workspace_changes_keep_thread_active_until_next_turn() {
        let manager = test_manager();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_workspace_changed(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WorkspaceChanged],
            },
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![],
            },
        );

        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;
        wait_for_status(&manager, INTERACTIVE_THREAD_ID, ThreadStatus::Idle).await;
    }

    #[tokio::test]
    async fn status_change_emits_notification() {
        let (manager, mut outgoing_rx) = test_manager_with_outgoing();

        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Idle,
            },
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Active {
                    active_flags: vec![],
                },
            },
        );

        manager.remove_thread(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::NotLoaded,
            },
        );
    }

    #[tokio::test]
    async fn silent_upsert_skips_initial_notification() {
        let (manager, mut outgoing_rx) = test_manager_with_outgoing();

        manager
            .upsert_thread_silently(test_thread(
                INTERACTIVE_THREAD_ID,
                codex_app_server_protocol::SessionSource::Cli,
            ))
            .await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Idle,
        );
        assert!(
            timeout(Duration::from_millis(100), outgoing_rx.recv())
                .await
                .is_err(),
            "silent upsert should not emit thread/status/changed"
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Active {
                    active_flags: vec![],
                },
            },
        );
    }

    async fn wait_for_status(
        manager: &ThreadWatchManager,
        thread_id: &str,
        expected_status: ThreadStatus,
    ) {
        timeout(Duration::from_secs(1), async {
            loop {
                let status = manager.loaded_status_for_thread(thread_id).await;
                if status == expected_status {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for status");
    }

    async fn recv_status_changed_notification(
        outgoing_rx: &mut mpsc::Receiver<OutgoingEnvelope>,
    ) -> ThreadStatusChangedNotification {
        let envelope = timeout(Duration::from_secs(1), outgoing_rx.recv())
            .await
            .expect("timed out waiting for outgoing notification")
            .expect("outgoing channel closed unexpectedly");
        let OutgoingEnvelope::Broadcast { message } = envelope else {
            panic!("expected broadcast notification");
        };
        let OutgoingMessage::AppServerNotification(ServerNotification::ThreadStatusChanged(
            notification,
        )) = message
        else {
            panic!("expected thread/status/changed notification");
        };
        notification
    }

    fn test_manager() -> ThreadWatchManager {
        ThreadWatchManager::new_with_file_watcher(None, Arc::new(FileWatcher::noop()))
    }

    fn test_manager_with_outgoing() -> (ThreadWatchManager, mpsc::Receiver<OutgoingEnvelope>) {
        let (outgoing_tx, outgoing_rx) = mpsc::channel(8);
        (
            ThreadWatchManager::new_with_file_watcher(
                Some(Arc::new(OutgoingMessageSender::new(outgoing_tx))),
                Arc::new(FileWatcher::noop()),
            ),
            outgoing_rx,
        )
    }

    fn test_thread(thread_id: &str, source: codex_app_server_protocol::SessionSource) -> Thread {
        Thread {
            id: thread_id.to_string(),
            forked_from_id: None,
            preview: String::new(),
            ephemeral: false,
            model_provider: "mock-provider".to_string(),
            created_at: 0,
            updated_at: 0,
            status: ThreadStatus::NotLoaded,
            resident: false,
            path: None,
            cwd: PathBuf::from("/tmp"),
            cli_version: "test".to_string(),
            agent_nickname: None,
            agent_role: None,
            source,
            git_info: None,
            name: None,
            turns: Vec::new(),
        }
    }
}
